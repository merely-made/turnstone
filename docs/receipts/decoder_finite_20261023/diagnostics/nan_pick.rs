mod infer {pub use esp::infer::{InferError,OutputGrammar};}
#[path="/Users/markik/Code/worktrees/esp-cross-source-answers-20261022/crates/esp/src/infer/decoder/constraint.rs"] mod constraint;
fn main(){
 let t=tokenizers::Tokenizer::from_file("/Users/markik/Code/output/cross-source-answers-20261022/qwen2.5-1.5b-instruct/tokenizer.json").unwrap();
 let g:infer::OutputGrammar=serde_json::from_slice(&std::fs::read("/Users/markik/Code/output/decoder-finite-20261023/grammar.json").unwrap()).unwrap();
 let text=std::fs::read_to_string("/Users/markik/Code/output/requested-facts-20261023/debug-prefix.txt").unwrap();let mut c=constraint::Constraint::new(&g,&t,151936).unwrap();for &id in t.encode(text,false).unwrap().get_ids(){c.accept(id).unwrap();}
 let mut row=vec![0f32;151936];row[16]=f32::NAN;row[17]=2.;row[18]=1.;let result=c.mask(&mut row);let best=row.iter().enumerate().max_by(|a,b|a.1.total_cmp(b.1).then_with(||b.0.cmp(&a.0))).unwrap();println!("mask={result:?} selected={} finite={} score={}",best.0,best.1.is_finite(),best.1);
}
