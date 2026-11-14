import json, sys, torch
torch.set_num_threads(1)
buf = torch.tensor([1,-2,3,4], dtype=torch.int32, device='cpu')
alias = buf.view(4)
before = buf.clone()
def apply(budget):
    if buf.numel() * buf.element_size() > budget: return 'budget_exhausted'
    buf.copy_(buf * 2 + 1)
    return 'ok'
assert apply(15) == 'budget_exhausted' and torch.equal(before,buf)
assert apply(16) == 'ok' and alias.tolist() == [3,-3,7,9]
assert int(alias.sum()) == 16
# A small float32 inference operation exercises CPU dispatch beyond import.
x=torch.tensor([[1.,2.],[3.,4.]],dtype=torch.float32)
y=x @ torch.tensor([[2.,0.],[0.,2.]])
assert torch.equal(y,torch.tensor([[2.,4.],[6.,8.]]))
print(json.dumps({'status':'PASS','python':sys.version,'executable':sys.executable,'torch':torch.__version__,'torch_file':torch.__file__,'cpu_device':str(buf.device),'buffer_dtype':str(buf.dtype),'inference_dtype':str(y.dtype),'cxx11_abi':torch.compiled_with_cxx11_abi(),'hip':torch.version.hip,'cuda':torch.version.cuda,'gpu_available':torch.cuda.is_available(),'values':alias.tolist(),'sum':int(alias.sum()),'failure_unchanged':True}))
