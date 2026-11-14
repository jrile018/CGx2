import array, json, wgpu
adapters=wgpu.gpu.enumerate_adapters_sync()
print(json.dumps({'adapters':[dict(a.info) for a in adapters]}))
matches=[a for a in adapters if a.info['backend_type']=='Vulkan' and a.info['adapter_type']=='DiscreteGPU' and a.info['vendor_id']==0x1002 and a.info['device_id']==0x7550]
assert len(matches)==1, 'exact RX9070 XT Vulkan adapter required; CPU fallback forbidden'
a=matches[0]; d=a.request_device_sync()
# Integer affine transform and separate float32 multiply verify two shader paths.
shader=d.create_shader_module(code='''
@group(0) @binding(0) var<storage, read_write> ib: array<i32>;
@group(0) @binding(1) var<storage, read_write> fb: array<f32>;
@compute @workgroup_size(4)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
 let i=gid.x;
 if (i<4u) { ib[i]=ib[i]*2+1; fb[i]=fb[i]*2.0; }
}''')
ib=d.create_buffer_with_data(data=array.array('i',[1,-2,3,4]),usage=wgpu.BufferUsage.STORAGE|wgpu.BufferUsage.COPY_SRC)
fb=d.create_buffer_with_data(data=array.array('f',[1.,2.,3.,4.]),usage=wgpu.BufferUsage.STORAGE|wgpu.BufferUsage.COPY_SRC)
p=d.create_compute_pipeline(layout='auto',compute={'module':shader,'entry_point':'main'})
bg=d.create_bind_group(layout=p.get_bind_group_layout(0),entries=[{'binding':0,'resource':{'buffer':ib}},{'binding':1,'resource':{'buffer':fb}}])
enc=d.create_command_encoder(); cp=enc.begin_compute_pass(); cp.set_pipeline(p);cp.set_bind_group(0,bg);cp.dispatch_workgroups(1);cp.end();d.queue.submit([enc.finish()])
d.queue.on_submitted_work_done_sync()
iv=array.array('i');iv.frombytes(d.queue.read_buffer(ib).tobytes())
fv=array.array('f');fv.frombytes(d.queue.read_buffer(fb).tobytes())
assert list(iv)==[3,-3,7,9] and list(fv)==[2.,4.,6.,8.]
print(json.dumps({'status':'PASS','wgpu':wgpu.__version__,'adapter':dict(a.info),'kernel':'WGSL affine integer + f32 multiply','shape':[4],'precision':['i32','f32'],'int_values':list(iv),'float_values':list(fv),'synchronization':'queue.on_submitted_work_done_sync followed by blocking read_buffer','cpu_fallback':False}))
