mod rope;

use crdt_testdata::{load_testing_data, TestPatch};
use criterion::measurement::WallTime;
use criterion::{
    criterion_group, criterion_main, BenchmarkGroup, BenchmarkId, Criterion, Throughput,
};
use rope::{Downstream, Upstream};

const TRACES: &[&str] = &[
    "automerge-paper",
    "rustcode",
    "sveltecomponent",
    "seph-blog1",
];

fn upstream(c: &mut Criterion) {
    fn bench<R: Upstream>(group: &mut BenchmarkGroup<WallTime>, trace_file: &str) {
        let mut trace = load_testing_data(&format!("./traces/{trace_file}.json.gz"));

        if R::EDITS_USE_BYTE_OFFSETS {
            trace = trace.chars_to_bytes();
        }

        group.throughput(Throughput::Elements(trace.len() as u64));

        group.bench_function(BenchmarkId::new(trace_file, R::NAME), |b| {
            b.iter(|| {
                let mut rope = R::from_str(&trace.start_content);
                for txn in &trace.txns {
                    for TestPatch(pos, del, ins) in &txn.patches {
                        rope.replace(*pos..*pos + del, ins);
                    }
                }
                assert_eq!(rope.len(), trace.end_content.len());
            })
        });
    }

    for trace in TRACES {
        let mut group = c.benchmark_group("upstream");

        bench::<rope::Automerge>(&mut group, trace);
        bench::<cola::Replica>(&mut group, trace);
        bench::<rope::Dt>(&mut group, trace);
        bench::<rope::Yrs>(&mut group, trace);
    }
}

fn downstream(c: &mut Criterion) {
    fn bench<R: Downstream>(group: &mut BenchmarkGroup<WallTime>, trace_file: &str) {
        let mut trace = load_testing_data(&format!("./traces/{trace_file}.json.gz"));

        if R::EDITS_USE_BYTE_OFFSETS {
            trace = trace.chars_to_bytes();
        }

        group.throughput(Throughput::Elements(trace.len() as u64));

        let (crdt, updates) = R::upstream_updates(&trace);

        group.bench_function(BenchmarkId::new(trace_file, R::NAME), |b| {
            b.iter(|| {
                let mut crdt = crdt.clone();
                for update in &updates {
                    crdt.apply_update(update);
                }
                assert_eq!(crdt.len(), trace.end_content.len());
            })
        });
    }

    for trace in TRACES {
        let mut group = c.benchmark_group("downstream");

        // bench::<rope::Automerge>(&mut group, trace);
        // bench::<cola::Replica>(&mut group, trace);
        bench::<rope::Dt>(&mut group, trace);
        // bench::<rope::Yrs>(&mut group, trace);
    }
}

criterion_group!(benches, upstream, downstream);

criterion_main!(benches);
