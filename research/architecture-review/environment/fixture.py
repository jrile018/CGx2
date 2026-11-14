import array, json, sys
buf = array.array('i', [1, -2, 3, 4])
assert buf.itemsize == 4
alias = memoryview(buf)
def apply(budget):
    if len(buf) * 4 > budget:
        return 'budget_exhausted'
    buf[:] = array.array('i', [x * 2 + 1 for x in buf])
    return 'ok'
before = buf.tobytes()
assert apply(15) == 'budget_exhausted'
assert buf.tobytes() == before
assert apply(16) == 'ok'
assert list(alias) == [3, -3, 7, 9] and sum(alias) == 16
print('PASS values=[3,-3,7,9] sum=16 rejected_budget=15 accepted_budget=16 failure_unchanged=true')
print(json.dumps({'python':sys.version,'executable':sys.executable}))
