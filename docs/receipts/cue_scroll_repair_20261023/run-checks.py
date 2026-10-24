from pathlib import Path
import subprocess

out = Path('/home/markik/Code/output/genet-scroll-background-20261023')
gates = [
    ('turnstone-pending-vulkan', 'turnstone', ['test','--lib','--locked','--offline','-j2','cue_content::tests::','--','--include-ignored','--nocapture','--test-threads=1']),
    ('turnstone-history', 'turnstone', ['test','--lib','--locked','--offline','-j2','app::surface_control_tests::','--','--test-threads=1']),
    ('turnstone-inspector', 'turnstone', ['test','--lib','--locked','--offline','-j2','inspector','--','--test-threads=1']),
    ('turnstone-browse', 'turnstone', ['test','--lib','--locked','--offline','-j2','browse::tests::','--','--test-threads=1']),
    ('turnstone-all-targets', 'turnstone', ['check','--all-targets','--locked','--offline','-j2']),
    ('pelt-receiving-vulkan', 'mere', ['test','--locked','--offline','-j2','-p','pelt-core','--features','cue','--test','cue','--','--include-ignored','--test-threads=1','--nocapture']),
]
for name, family, args in gates:
    subprocess.run(['python3', str(out/'run-gate.py'), name, family, *args], check=True)
