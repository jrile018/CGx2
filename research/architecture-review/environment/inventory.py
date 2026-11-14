import ctypes, glob, json, os, platform, shutil, stat, subprocess
def output(args):
    p=subprocess.run(args,capture_output=True,text=True)
    return {'exit_code':p.returncode,'output':p.stdout+p.stderr}
devs={}
for p in glob.glob('/dev/dri/render*')+['/dev/kfd']:
    if not os.path.exists(p): continue
    s=os.stat(p)
    try:
        fd=os.open(p,os.O_RDWR);os.close(fd);access='open_RDWR_pass'
    except OSError as e: access=f'{type(e).__name__}: {e.strerror}'
    devs[p]={'mode':stat.filemode(s.st_mode),'gid':s.st_gid,'access':access}
pci={}
for p in glob.glob('/sys/class/drm/card[0-9]/device/uevent'):
    pci[p]={k:v for k,v in (x.split('=',1) for x in open(p).read().splitlines()) if k in ['DRIVER','PCI_ID','PCI_SLOT_NAME']}
lib=output(['ldconfig','-p'])
libs=[x.strip() for x in lib['output'].splitlines() if any(y in x.lower() for y in ['libamdhip','libhsa','libopencl','libvulkan'])]
ocl=ctypes.CDLL('libOpenCL.so.1');count=ctypes.c_uint(0)
err=ocl.clGetPlatformIDs(0,None,ctypes.byref(count))
load_attempts={}
for library in ['libamdhip64.so','libhsa-runtime64.so.1']:
    try: ctypes.CDLL(library); load_attempts[library]='loaded'
    except OSError as e: load_attempts[library]=str(e)
cpu=next((x.split(':',1)[1].strip() for x in open('/proc/cpuinfo') if x.startswith('model name')),'unknown')
print(json.dumps({'machine':platform.machine(),'kernel':platform.release(),'cpu':cpu,'logical_cpus':os.cpu_count(),'devices':devs,'pci':pci,'compute_libraries':libs,'HIP_HSA_load_attempts':load_attempts,'opencl':{'clGetPlatformIDs_return':err,'platform_count':count.value,'ICD_files':glob.glob('/etc/OpenCL/vendors/*')},'tools':{x:shutil.which(x) for x in ['rustc','cargo','python3','uv','c++','cmake','hipcc','rocminfo','clinfo','vulkaninfo']},'versions':{x:output(args) for x,args in {'rustc':['rustc','--version'],'cargo':['cargo','--version'],'system_python':['python3','--version'],'ocamlopt':['/home/john-riley/.opam/serena-ocaml/bin/ocamlopt','-version'],'c++':['c++','--version']}.items()}},indent=2))
