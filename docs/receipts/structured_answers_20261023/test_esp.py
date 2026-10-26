from pathlib import Path
import subprocess,os,shutil
O=Path(__file__).resolve().parent;E=Path('/Users/markik/Code/worktrees/esp-cross-source-answers-20261022');original=(E/'Cargo.lock').read_bytes();shutil.copy2(O/'esp-tested.Cargo.lock',E/'Cargo.lock')
env=os.environ.copy();env.update(TMPDIR='/tmp',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_INCREMENTAL='0')
base=['cargo','--config',str(O/'optimized-runtime.toml'),'--config','build.target-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-target"','--config','build.build-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-build"','test','--locked','-j4','-p','esp','--features','decoder,model-session']
try:
 for args in [['--lib','infer'],['--test','prompt_template']]:subprocess.run(base+args,cwd=E,env=env,check=True)
finally:(E/'Cargo.lock').write_bytes(original)
