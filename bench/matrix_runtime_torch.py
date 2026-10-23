#!/usr/bin/env python3
"""Optional direct Python torch.matmul worker for matrix_runtime.py."""
import argparse
import json
import os
from pathlib import Path
import resource
import time


def inputs(m, k, n):
    if (m, k, n) == (2, 3, 2):
        return [1., 2., 3., 4., 5., 6.], [7., 8., 9., 10., 11., 12.]
    return ([(i % 7 - 3) / 8 for i in range(m * k)],
            [(i % 5 - 2) / 7 for i in range(k * n)])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('phase', choices=['cold', 'warm'])
    parser.add_argument('m', type=int)
    parser.add_argument('k', type=int)
    parser.add_argument('n', type=int)
    parser.add_argument('warmup', type=int)
    parser.add_argument('reps', type=int)
    args = parser.parse_args()
    if os.environ.get('OMP_NUM_THREADS') != '1' or os.environ.get('MKL_NUM_THREADS') != '1':
        raise SystemExit('OMP_NUM_THREADS and MKL_NUM_THREADS must be 1')
    import torch
    if torch.__version__.split('+')[0] != '2.11.0' or torch.version.cuda is not None:
        raise SystemExit('requires Python Torch 2.11.0 CPU')
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    left, right = inputs(args.m, args.k, args.n)

    def call():
        a = torch.tensor(left, dtype=torch.float64, device='cpu').reshape(args.m, args.k)
        b = torch.tensor(right, dtype=torch.float64, device='cpu').reshape(args.k, args.n)
        begin = time.perf_counter_ns()
        c = torch.matmul(a, b)
        matmul_ns = time.perf_counter_ns() - begin
        return c.reshape(-1).tolist(), matmul_ns

    output, _ = call()
    result = {'version': torch.__version__.split('+')[0],
              'torch_root': str(Path(torch.__file__).resolve().parent), 'output': output}
    if args.phase == 'warm':
        for _ in range(args.warmup):
            call()
        samples = []
        for _ in range(args.reps):
            cpu_start = time.process_time_ns()
            wall_start = time.perf_counter_ns()
            got, matmul_ns = call()
            wall_ns = time.perf_counter_ns() - wall_start
            if any(abs(a - b) > 1e-9 for a, b in zip(got, output)) or len(got) != len(output):
                raise SystemExit('output changed')
            samples.append({'wall_ns': wall_ns, 'cpu_ns': time.process_time_ns() - cpu_start,
                            'matmul_wall_ns': matmul_ns,
                            'peak_rss_kib': resource.getrusage(resource.RUSAGE_SELF).ru_maxrss})
        result['samples'] = samples
    print(json.dumps(result, separators=(',', ':')))


if __name__ == '__main__':
    main()
