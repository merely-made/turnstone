from pathlib import Path
import subprocess
out=Path('/home/markik/Code/output/cue-native-vulkan-20261023')
gates=[
 ('reader-content-pool-final','turnstone',['test','--lib','--features','servo','--locked','--offline','-j2','shell::content_pool::tests::','--','--nocapture','--test-threads=1']),
 ('turnstone-pending-vulkan','turnstone',['test','--lib','--features','servo','--locked','--offline','-j2','cue_content::tests::','--','--include-ignored','--nocapture','--test-threads=1']),
 ('turnstone-default-all-targets','turnstone',['check','--all-targets','--locked','--offline','-j2']),
 ('turnstone-servo-all-targets','turnstone',['check','--all-targets','--features','servo','--locked','--offline','-j2']),
 ('pelt-vulkan-final','mere',['test','--locked','--offline','-j2','-p','pelt-core','--features','cue','--test','cue','--','--include-ignored','--test-threads=1','--nocapture'])
]
for name,family,args in gates:
 subprocess.run(['python3',str(out/'run-gate.py'),name,family,*args],check=True)
