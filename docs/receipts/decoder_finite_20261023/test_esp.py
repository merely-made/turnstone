from pathlib import Path
import subprocess,os,shutil
O=Path(__file__).resolve().parent;E=Path('/Users/markik/Code/worktrees/esp-cross-source-answers-20261022');original=(E/'Cargo.lock').read_bytes();lock=(O.parent/'structured-answers-20261023/esp-tested.Cargo.lock').read_text().replace('44f0664748238d7a96603925a0268caf60e091aa','dceb5fcd07bfaeffc57142b06f07259eed0e89ef');(E/'Cargo.lock').write_text(lock)
env=os.environ.copy();env.update(RUSTC='/usr/local/bin/rustc',RUSTDOC='/usr/local/bin/rustdoc',TMPDIR='/tmp',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_INCREMENTAL='0',ESP_SCORE_TRACE_ROOT=str(O),ESP_SCORE_TRACE_MODEL=str(O.parent/'cross-source-answers-20261022/qwen2.5-1.5b-instruct'),ESP_SCORE_TRACE_MODE=os.environ.get('ESP_SCORE_TRACE_MODE','block'))
base=['/usr/local/bin/cargo','--config',str(O.parent/'streamed-inference-20261022/optimized-runtime.toml'),'--config','build.target-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-target"','--config','build.build-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-build"','test','--locked','-j4','-p','esp','--features','decoder-wgpu,model-session']
cmd=base+['--lib','infer']

try:
 subprocess.run(cmd,cwd=E,env=env,check=True)
 subprocess.run(base+['--test','prompt_template'],cwd=E,env=env,check=True)
finally:(E/'Cargo.lock').write_bytes(original)
