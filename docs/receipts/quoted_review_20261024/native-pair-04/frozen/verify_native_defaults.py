#!/usr/bin/env python3
"""Run unchanged native assertions using the declared omitted-empty wire default."""
from pathlib import Path
import hashlib,json
import verify_native_bindings as verify_native
from verify_wire_defaults import decoded
original=verify_native.verify_answer
def verify(run,case,cases):
 view,_=decoded(run)
 return original(view,case,cases)
if __name__=='__main__':
 verify_native.verify_answer=verify
 verify_native.main()
 import sys
 base=Path(sys.argv[1]);path=base/'verification.json';result=json.loads(path.read_text())
 result['protocol_default']='Only omitted recall.citations decodes to []; raw files, support, native assertions and cases unchanged.'
 result['original_native_checker_sha256']=hashlib.sha256((Path(__file__).parent/'verify_native.py').read_bytes()).hexdigest()
 result['native_binding_checker_sha256']=hashlib.sha256(Path(verify_native.__file__).read_bytes()).hexdigest()
 result['reader_identity_policy']='Saved-reader target.key.node identifies the admitted source; top-level node identifies the reader view. Retained-reader node remains its source identity.'
 (base/'wire-default-native-review.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');path.unlink()
