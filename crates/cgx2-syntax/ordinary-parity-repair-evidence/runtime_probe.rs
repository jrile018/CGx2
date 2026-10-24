//! Direct public runtime evidence, never labeled source acceptance.
use cgx2_runtime::{AllocationError as A, CollectionMode as M, Handle, MemoryError, Session,
                   REGION_METADATA_BYTES, SLOT_METADATA_BYTES};
use std::mem::size_of;
use std::panic::{catch_unwind, AssertUnwindSafe};
fn row(name: &str, passed: bool, details: impl std::fmt::Debug) {
    println!("{{\"name\":{:?},\"passed\":{},\"details\":{:?}}}", name, passed, format!("{details:?}"));
}
fn budget_cases() {
    let alloc = SLOT_METADATA_BYTES + 8;
    let root = size_of::<Handle>();
    for (label, limit) in [("zero",0),("exact",alloc),("one-under",alloc-1)] {
        let mut s=Session::try_new().unwrap(); let r=s.default_region();
        let result={let mut g=s.try_no_gc(limit).unwrap(); g.alloc_bytes(r,8)};
        row(&format!("allocation-{label}"), result.is_ok()==(label=="exact") &&
            (label=="exact" || result==Err(A::BudgetExceeded)), result);
    }
    for (label, limit) in [("zero",0),("exact",alloc),("one-under",alloc-1)] {
        let mut s=Session::try_new().unwrap(); let r=s.create_region(M::Manual,limit).unwrap();
        let result=s.alloc_bytes(r,8);
        row(&format!("region-limit-{label}"), result.is_ok()==(label=="exact") &&
            (label=="exact" || result==Err(A::RegionLimit)), result);
    }
    for (label, limit) in [("zero",0),("exact",REGION_METADATA_BYTES),("one-under",REGION_METADATA_BYTES-1)] {
        let mut s=Session::try_new().unwrap();
        let result={let mut g=s.try_no_gc(limit).unwrap(); g.create_region(M::Manual,4096)};
        row(&format!("region-bookkeeping-{label}"), result.is_ok()==(label=="exact") &&
            (label=="exact" || result==Err(A::BudgetExceeded)), result);
    }
    for (label, limit) in [("zero",0),("exact",root),("one-under",root-1)] {
        let mut s=Session::try_new().unwrap(); let r=s.default_region(); let h=s.alloc_bytes(r,8).unwrap();
        let mut g=s.try_no_gc(limit).unwrap();
        let mut roots=g.scoped_roots(); let result=roots.root(h); let clean=roots.cleanup();
        row(&format!("root-bookkeeping-{label}"), result.is_ok()==(label=="exact") &&
            (label=="exact" || result==Err(MemoryError::Allocation(A::BudgetExceeded))) &&
            clean.first_error.is_none() && clean.released_roots==usize::from(label=="exact"), (result, clean));
    }
    // Budget is two usize fields; nested entry replaces a 1-slot table with 2 slots.
    let guard=4*size_of::<usize>();
    for (label, limit) in [("zero",0),("exact",guard),("one-under",guard-1)] {
        let mut s=Session::try_new().unwrap(); let mut outer=s.try_no_gc(limit).unwrap();
        let result=match outer.try_no_gc(0) {Ok(_)=>Ok(()),Err(e)=>Err(e)};
        row(&format!("guard-bookkeeping-{label}"), result.is_ok()==(label=="exact") &&
            (label=="exact" || result==Err(A::BudgetExceeded)), result);
    }
    // One-node raw byte copy: temporary root + traversal vector + HashMap(3 buckets)
    // + owned encoded image(12+8+32) + staging vector + payload/slot + staging root.
    // Explicitly pinned to Rust 1.98.1's 3-bucket initial HashMap capacity.
    let copy=4*root + 3*(size_of::<(Handle,u32)>()+1) + 12+8+32 + alloc;
    for (label,limit) in [("zero",0),("exact",copy),("one-under",copy-1)] {
        let mut s=Session::try_new().unwrap(); let source=s.default_region();
        let destination=s.create_region(M::Manual,4096).unwrap();
        let h=s.alloc_bytes(source,8).unwrap(); s.root(h).unwrap();
        let before=s.memory_observation_v2().charged_bytes;
        let (ok,err,clean)={let mut g=s.try_no_gc(limit).unwrap(); let mut roots=g.scoped_roots();
            let result=roots.copy_graph_to_v1(destination,h);
            (result.is_ok(),format!("{result:?}"),roots.cleanup())};
        let live=s.stats(destination).unwrap().committed_bytes;
        let valid=s.read_bytes(h).is_ok(); s.unroot(h).unwrap();
        row(&format!("copy-budget-{label}"), ok==(label=="exact") && valid &&
            (label=="exact" || (err.contains("BudgetExceeded") && live==0)) &&
            (label!="exact" || s.memory_observation_v2().charged_bytes-before==(copy+2*size_of::<usize>()) as u128) &&
            clean.first_error.is_none() && s.root_census().unwrap().root_registrations==0,
            (copy,err,live,clean,s.memory_observation_v2().charged_bytes-before));
    }
    let mut s=Session::try_new().unwrap();
    {let mut outer=s.try_no_gc(2*REGION_METADATA_BYTES+2*alloc-1+guard).unwrap();
     let mut inner=outer.try_no_gc(2*REGION_METADATA_BYTES+2*alloc-1).unwrap();
     let a=inner.create_region(M::Manual,4096).unwrap(); let b=inner.create_region(M::Manual,4096).unwrap();
     let h=inner.alloc_bytes(a,8).unwrap(); inner.close_region(a).unwrap();
     let second=inner.alloc_bytes(b,8);
     row("nested-multiple-regions-cumulative-nonrefund", second==Err(A::BudgetExceeded) &&
         inner.read_bytes(h).is_err() && inner.stats(b).unwrap().collection_count==0, second);
    }
    row("manual-guard-exit-never-collects",s.memory_observation_v2().collection_count==0 &&
        s.memory_observation_v2().active_guards==0,s.memory_observation_v2());
}
fn sweep(index: usize) -> usize {
    let mut s=Session::try_new().unwrap(); let baseline=s.root_census().unwrap();
    s.inject_host_failure_at_reserve(index);
    let mut destination=None; let mut source=None;
    let result=(|| -> Result<(),String> {
        let mut guard=s.try_no_gc(65536).map_err(|e|format!("{e:?}"))?;
        let a=guard.create_region(M::Manual,4096).map_err(|e|format!("{e:?}"))?;
        let b=guard.create_region(M::Manual,4096).map_err(|e|format!("{e:?}"))?; destination=Some(b);
        let mut roots=guard.scoped_roots();
        let h=roots.alloc_object(a,8,1).map_err(|e|format!("{e:?}"))?; source=Some(h);
        roots.root(h).map_err(|e|format!("{e:?}"))?;
        roots.set_reference(h,0,Some(h)).map_err(|e|format!("{e:?}"))?;
        let copy=roots.copy_graph_to_v1(b,h).map_err(|e|format!("{e:?}"))?;
        if roots.reference(copy,0)!=Ok(Some(copy)) {return Err("topology changed".into())}
        let clean=roots.cleanup();
        if clean.first_error.is_some() {return Err(format!("cleanup {clean:?}"))}
        Ok(())
    })();
    let attempts=s.host_reserve_attempts(); let site=s.last_host_failure_site_v2();
    let final_census=s.root_census().unwrap();
    let source_valid=source.map(|h|s.read_bytes(h).is_ok()).unwrap_or(true);
    let destination_bytes=destination.map(|r|s.stats(r).unwrap().committed_bytes).unwrap_or(0);
    let atomic=result.is_ok() || destination_bytes==0;
    let passed=baseline==final_census && source_valid && atomic &&
        (if site.is_some(){result.as_ref().is_err_and(|e|e.contains("HostAllocation"))}else{result.is_ok()}) &&
        s.memory_observation_v2().collection_count==0 && s.memory_observation_v2().active_guards==0;
    row(&format!("reserve-{index}"),passed,(attempts,site,result,baseline,final_census,source_valid,destination_bytes));
    attempts
}
fn cleanup() {
    for primary in ["normal","return","failed-argument","contract","arithmetic","allocation","cancellation"] {
        let mut s=Session::try_new().unwrap(); let r=s.create_region(M::Manual,4096).unwrap();
        let baseline=s.root_census().unwrap();
        let mut roots=s.scoped_roots(); let h=roots.alloc_bytes(r,8).unwrap(); roots.root(h).unwrap();
        let result:Result<(),&str>=if primary=="normal" {Ok(())}else{Err(primary)};
        let outcome=roots.finish(result); let again=roots.cleanup(); drop(roots);
        row(&format!("library-cleanup-{primary}"),outcome.result==result && outcome.cleanup.first_error.is_none() &&
            outcome.cleanup.released_roots==1 && again.released_roots==0 && baseline==s.root_census().unwrap() &&
            s.stats(r).unwrap().collection_count==0,(outcome,again));
    }
    let mut cancelled=Session::try_new().unwrap(); let r=cancelled.default_region();
    let baseline=cancelled.root_census().unwrap();
    {let mut state=cancelled.scoped_roots();let h=state.alloc_bytes(r,8).unwrap();state.root(h).unwrap();
     drop(state);}
    row("library-cooperative-cancellation-drop",baseline==cancelled.root_census().unwrap(),cancelled.root_census());
    let mut s=Session::try_new().unwrap(); let r=s.default_region(); let baseline=s.root_census().unwrap();
    let panic=catch_unwind(AssertUnwindSafe(|| {let mut roots=s.scoped_roots();let h=roots.alloc_bytes(r,8).unwrap();
        roots.root(h).unwrap(); panic!("bounded unwind sentinel");}));
    row("library-panic-unwind",panic.is_err() && baseline==s.root_census().unwrap(),s.root_census());
    let mut s=Session::try_new().unwrap(); let r=s.create_region(M::Auto{trigger_bytes:1},4096).unwrap();
    let mut roots=s.scoped_roots(); let h=roots.alloc_bytes(r,8).unwrap();
    let result=roots.copy_graph_to_v1(r,h);
    let valid=roots.read_bytes(h).is_ok();
    row("OM06-safe-same-region-copy",result.is_ok() && valid,(result,valid,roots.stats(r)));
}
fn main() {
    budget_cases(); cleanup();
    let entry=Session::try_new_with_host_failure_at_reserve(1);
    row("session-entry-reserve-1",entry.is_err(),entry.err());
    let bound=sweep(usize::MAX);
    row("sweep-bound-at-most-64",bound<=64,bound);
    for index in 1..=bound+1 {sweep(index);}
}
