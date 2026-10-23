//! Isolated streaming-hash buffer experiment, not a replacement loader route.
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::time::Instant;

fn main() -> std::io::Result<()> {
    let path = std::env::args().nth(1).expect("ELF path required");
    let mut file = File::open(path)?;
    let mut expected = None;
    // Fixed round-robin order, five observations per size. Same descriptor,
    // same SHA implementation, whole-file read through EOF every time.
    for repetition in 0..5 {
        for size in [64 * 1024, 256 * 1024, 1024 * 1024] {
            let start = Instant::now();
            file.seek(SeekFrom::Start(0))?;
            let mut digest = Sha256::new();
            let mut buffer = vec![0; size];
            let mut bytes = 0;
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                digest.update(&buffer[..read]);
                bytes += read;
            }
            let hash = format!("{:x}", digest.finalize());
            let ns = start.elapsed().as_nanos();
            if let Some(ref expected) = expected {
                assert_eq!(expected, &hash);
            } else {
                expected = Some(hash.clone());
            }
            println!(
                "{{\"repetition\":{repetition},\"buffer_bytes\":{size},\"bytes\":{bytes},\"ns\":{ns},\"sha256\":\"{hash}\"}}"
            );
        }
    }
    Ok(())
}
