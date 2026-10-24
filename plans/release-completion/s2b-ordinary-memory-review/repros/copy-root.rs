use cgx2_runtime::{Session, CollectionMode};
fn main() {
    let mut session = Session::try_new().unwrap();
    let region = session.create_region(CollectionMode::Auto { trigger_bytes: 1 }, 1 << 20).unwrap();
    let mut roots = session.scoped_roots();
    let source = roots.alloc_bytes(region, 8).unwrap();
    roots.write_bytes(source, 0, &42i64.to_le_bytes()).unwrap();
    let copied = roots.copy_graph_to_v1(region, source).unwrap();
    println!("source={:?}, copied={:?}, collections={}", roots.read_bytes(source), roots.read_bytes(copied), roots.stats(region).unwrap().collection_count);
}
