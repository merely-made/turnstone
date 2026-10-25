from pathlib import Path
import subprocess
out=Path('/home/markik/Code/output/cue-native-vulkan-20261023')
gates=[
 ('turnstone-servo-library','turnstone',['test','--lib','--features','servo','--locked','--offline','-j2','--','--test-threads=1']),
 ('turnstone-pending-vulkan','turnstone',['test','--lib','--features','servo','--locked','--offline','-j2','cue_content::tests::','--','--include-ignored','--nocapture','--test-threads=1']),
 ('turnstone-default-all-targets','turnstone',['check','--all-targets','--locked','--offline','-j2']),
 ('turnstone-servo-all-targets','turnstone',['check','--all-targets','--features','servo','--locked','--offline','-j2']),
]
for name,family,args in gates:
 subprocess.run(['python3',str(out/'run-gate.py'),name,family,*args],check=True)
