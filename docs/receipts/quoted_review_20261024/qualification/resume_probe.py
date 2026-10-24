"""Resume after successful software checks/publication; keep cargo stderr separate.
The first orchestration captured a non-fatal Cargo diagnostic in JSON stdout.
No model run had begun and no model output is repaired here.
"""
from pathlib import Path
import json
O=Path(__file__).resolve().parent
source=(O/'qualify.py').read_text();scope={'__file__':str(O/'qualify.py')}
exec(source[:source.index("phase('awaiting final resident tests')")],scope)
scope['pins']=json.loads((O/'supplier-pins.json').read_text())
scope['binary']=Path((O/'resident-binary-path').read_text().strip())
exec(source[source.index('canonical={}'):],scope)
