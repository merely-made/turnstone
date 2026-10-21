#!/usr/bin/env python3
"""Match every compiled source input to immutable Git blobs."""
import argparse, hashlib, json, subprocess
from pathlib import Path
HERE=Path(__file__).resolve().parent
def main():
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--supplier',type=Path,required=True)
 p.add_argument('--supplier-commit',required=True)
 p.add_argument('--turnstone-commit',default='a048adcc36eef8597ed913940d83103b9eae1633')
 a=p.parse_args();inputs=json.loads((HERE/'native/inputs.json').read_text())
 for root,commit,field in [(HERE.parents[2],a.turnstone_commit,'source_sha256'),(a.supplier,a.supplier_commit,'supplier_source_sha256')]:
  for path,expected in inputs[field].items():
   blob=subprocess.check_output(['git','show',commit+':'+path],cwd=root)
   assert hashlib.sha256(blob).hexdigest()==expected,(commit,path)
 result={'result':'pass','turnstone_compiled_source_commit':a.turnstone_commit,'turnstone_source_files_checked':len(inputs['source_sha256']),'resident_compiled_source_commit':a.supplier_commit,'supplier_inputs_checked':len(inputs['supplier_source_sha256']),'all_source_hashes_match_git_blobs':True}
 (HERE/'compiled-source-proof.json').write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps(result,indent=2))
if __name__=='__main__':main()
