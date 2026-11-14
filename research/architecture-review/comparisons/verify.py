#!/usr/bin/env python3
"""Offline comparison-spec checks; does not execute workload/model programs."""
import hashlib
import itertools
import json
import math
from collections import Counter
from pathlib import Path

LANE=Path(__file__).resolve().parent
ROOT=LANE.parents[2]

def require(value,message):
    if not value:
        raise ValueError(message)

def main():
    m=json.loads((LANE/'matrix.json').read_text())
    dimensions={'languages':['CGx2','Rust','OCaml','Python'],
                'policies':['idiomatic','equipped'], 'targets':['cpu','gpu'],
                'tracks':['native','data_ml','general_llm']}
    for key,values in dimensions.items():
        require(m[key]==values,f'wrong {key}')
    require(m['status']=='specified_not_executed','incorrect evidence status')
    for ref in m['input_refs']:
        path=Path(ref['path'])
        require(not path.is_absolute() and '..' not in path.parts,'unsafe input path')
        require(hashlib.sha256((ROOT/path).read_bytes()).hexdigest()==ref['sha256'],f'input drift: {path}')
    require(len(m['cases'])>=9,'too few cases')
    require({'N01','N02','N03','M01','M02','M03','G01','G02','G03'} <= {c['id'] for c in m['cases']},'required workload missing')
    require(set(c['track'] for c in m['cases'])==set(m['tracks']),'missing track')
    case_ids=set(); route_ids=set(); counts=Counter(); track_counts=Counter()
    expected=set(itertools.product(m['languages'],m['policies'],m['targets']))
    for c in m['cases']:
        require(c['id'] not in case_ids,'duplicate case id'); case_ids.add(c['id'])
        require(c['track'] in m['tracks'],'unknown track'); track_counts[c['track']]+=1
        for key in ['name','behavior','inputs','errors','dtype','cost_boundary','aliasing_lifetime','acceptance_tests','metrics','policy_contracts']:
            require(c.get(key),f'missing case {key}: {c["id"]}')
        require(set(c['applicability'])==set(m['targets']),'missing target applicability')
        for target,app in c['applicability'].items():
            require(type(app['applicable']) is bool and app['reason'],'missing applicability reason')
        require(len(c['routes'])==len(expected),'incomplete route count')
        actual=set()
        for r in c['routes']:
            combination=(r['language'],r['policy'],r['target'])
            require(combination in expected and combination not in actual,'duplicate/invalid route dimension'); actual.add(combination)
            require(r['id']==':'.join([c['id'],*combination]),'noncanonical route id')
            require(r['id'] not in route_ids,'duplicate route id'); route_ids.add(r['id'])
            require(r['status'] in {'not_run','blocked','not_applicable'},'unrun comparison claimed pass/result')
            require(r['applicable']==c['applicability'][r['target']]['applicable'],'applicability mismatch')
            require((r['status']=='not_applicable')==(not r['applicable']),'invalid not_applicable status')
            require(r['reason'] and r['backend'] and r['implementation_profile'],'missing route boundary')
            require(r['measurements'] is None and r['evidence']==[],'unrun route contains result evidence')
            require(r['readiness_is_comparison_evidence'] is False,'readiness promoted to comparison')
            require(bool(r['blocker_ids'])==(r['status']=='blocked'),'blocked reason missing or spurious')
            require(all(b in m['blocker_definitions'] for b in r['blocker_ids']),'unknown blocker')
            require(r['policy_negative_tests'] in {'required','not_applicable'},'policy contract missing')
            require((r['policy_negative_tests']=='required')==(r['policy']=='equipped'),'policy-stratum mismatch')
            counts[r['status']]+=1
        require(actual==expected,'incomplete Cartesian route matrix')
    for id in ['M01','M02','M04']:
        c=next(c for c in m['cases'] if c['id']==id)
        require(c['applicability']['gpu']['applicable'],'device workload GPU row retired')
    s=m['study']; require(s['status']=='not_run','study result invented')
    require(set(s['families'])=={'bugfix','refactor','author'},'study families missing')
    require(s['model_families_minimum']>=2,'single-family study')
    for phase in ['pilot','holdout']:
        p=s[phase]
        require(p['attempts']==p['task_slots']*p['languages_per_task']*p['model_families']*p['repetitions'],'study count arithmetic')
    budget=s['attempt_budget']; cost=s['cost_proposal']
    require(cost['attempts']==s['pilot']['attempts']+s['holdout']['attempts'],'cost total attempts')
    for option in cost['options']:
        each=(budget['input_tokens']*option['input_usd_per_million']+budget['output_tokens']*option['output_usd_per_million'])/1_000_000
        for phase in ['pilot','holdout']:
            require(math.isclose(option[phase+'_usd'],each*s[phase]['attempts'],abs_tol=1e-6),'cost phase arithmetic')
        require(math.isclose(option['full_usd'],each*cost['attempts'],abs_tol=1e-6),'full cost arithmetic')
        require(math.isclose(option['budget_ceiling_usd'],option['full_usd']*(1+option['reserve_fraction']),abs_tol=1e-6),'reserve arithmetic')
    for path in LANE.iterdir():
        if path.is_file():
            for number,line in enumerate(path.read_text().splitlines(),1):
                require(line.rstrip()==line,f'trailing whitespace: {path.name}:{number}')
    print(json.dumps({'status':'pass','input_hashes':len(m['input_refs']), 'cases':len(case_ids),
        'tracks':dict(track_counts),'route_rows':len(route_ids),'route_states':dict(counts),
        'study_attempts_specified':cost['attempts'],'executed_comparisons':0,'executed_model_trials':0,
        'cost_arithmetic':'pass; assumed ceilings, not vendor quotes','scope':'specification/schema/input provenance only; no workload acceptance, performance, proof or authoring outcomes'},indent=2))

if __name__=='__main__':
    main()
