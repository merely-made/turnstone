mod infer { pub use esp::infer::{InferError, OutputGrammar}; }
#[path="/Users/markik/Code/worktrees/esp-cross-source-answers-20261022/crates/esp/src/infer/decoder/constraint.rs"] mod constraint;
use esp::infer::decoder::{DecoderConfig, DecoderModel, DecoderDevice, DecoderGpuKind, checkpoint::CheckpointReader, loader, streaming::{self, StreamingOptions}};
use burn::tensor::{Tensor, TensorData, Int};
use std::sync::{Arc,Mutex};
use std::io::Write;
fn main() {
 let args:Vec<_>=std::env::args().collect();let root=std::path::Path::new(&args[1]);let mode=args.get(2).map(String::as_str).unwrap_or("block");
 let directory=std::path::Path::new("/Users/markik/Code/output/cross-source-answers-20261022/qwen2.5-1.5b-instruct");
 let config=DecoderConfig::from_json_bytes(&std::fs::read(directory.join("config.json")).unwrap()).unwrap();
 let tokenizer=tokenizers::Tokenizer::from_file(directory.join("tokenizer.json")).unwrap();
 let options=StreamingOptions::default();let mut reader=CheckpointReader::open(directory,&config).unwrap();
 let shared=reader.shared_bytes(options.max_shared_bytes).unwrap();let tensors=safetensors::SafeTensors::deserialize(&shared).unwrap();
 let device=if mode=="cpu" {DecoderDevice::ndarray()} else {DecoderDevice::wgpu(DecoderGpuKind::DiscreteGpu(0))};
 let reader=Arc::new(Mutex::new(reader));let source=move |index:usize,limit:usize|reader.lock().unwrap().layer_bytes(index,limit);
 let source=streaming::cache_verified_layers(Arc::new(source),3*1024usize.pow(3));
 let model=DecoderModel::from_layer_source(config.clone(),{
let decode=|name:&str|{let v=tensors.tensor(name).unwrap();assert_eq!(v.dtype(),safetensors::Dtype::BF16);v.data().chunks_exact(2).map(|b|f32::from_bits((u16::from_le_bytes([b[0],b[1]]) as u32)<<16)).collect::<Vec<_>>()};
esp::infer::decoder::LoadedDecoder {embed_w:Tensor::from_data(TensorData::new(decode("model.embed_tokens.weight"),[config.vocab_size,config.hidden_size]),&device),layers:vec![],final_norm_gamma:Tensor::from_data(TensorData::new(decode("model.norm.weight"),[config.hidden_size]),&device),lm_head_w:None}
},Box::new(source),options,&device).unwrap().with_layer_cache(4*1024u64.pow(3)).unwrap();
 let grammar:infer::OutputGrammar=serde_json::from_slice(&std::fs::read(root.join("grammar.json")).unwrap()).unwrap();
 let mut constraint=constraint::Constraint::new(&grammar,&tokenizer,config.vocab_size).unwrap();
 let prompt=std::fs::read_to_string(root.join("rendered-prompt.txt")).unwrap();let mut prefix=tokenizer.encode(prompt,false).unwrap().get_ids().to_vec();
 let mut cache=model.new_cache();let mut generated=vec![];let mut last_block=prefix.len();
 let forward=|ids:&[u32],cache:&mut esp::infer::decoder::KvCache|model.try_forward_cached_last(Tensor::<2,Int>::from_data(TensorData::new(ids.iter().map(|&x|x as i32).collect::<Vec<_>>(),[1,ids.len()]),&device),cache).unwrap();
 let mut logits=forward(&prefix,&mut cache);
 let mut file=std::fs::File::create(root.join(format!("score-trace-{mode}.jsonl"))).unwrap();
 for step in 0..512 {
  let forced=constraint.forced(&tokenizer).unwrap();
  let mut row=if forced.is_empty(){logits.clone().into_data().try_to_vec::<f32>().unwrap()}else{vec![]};
  let invalid:Vec<_>=row.iter().enumerate().filter(|(_,x)|!x.is_finite()).map(|(i,_)|i).collect();
  let real_invalid=invalid.iter().filter(|&&id|tokenizer.id_to_token(id as u32).is_some()).count();
  writeln!(file,"{}",serde_json::json!({"step":step,"prefix_tokens":prefix.len(),"output_tokens":generated.len(),"last_forward_block":last_block,"next_forced_tokens":forced.len(),"nonfinite":invalid.len(),"real_nonfinite":real_invalid,"nan":row.iter().filter(|v|v.is_nan()).count(),"first_invalid":invalid.iter().take(16).collect::<Vec<_>>(),"row_read":!row.is_empty()})).unwrap();file.flush().unwrap();
  if real_invalid>0 {
   std::fs::write(root.join(format!("first-invalid-{mode}.json")),serde_json::to_vec_pretty(&serde_json::json!({"prefix_ids":prefix,"output_ids":generated,"decoded":tokenizer.decode(&generated,false).unwrap(),"last_forward_block":last_block})).unwrap()).unwrap();break;
  }
  let ids=if forced.is_empty() {
   constraint.mask(&mut row).unwrap();vec![row.iter().enumerate().max_by(|a,b|a.1.total_cmp(b.1).then_with(||b.0.cmp(&a.0))).unwrap().0 as u32]
  } else {forced};
  for &id in &ids {constraint.accept(id).unwrap();generated.push(id);prefix.push(id);}
  if constraint.complete(){println!("complete {}",generated.len());break;}
  last_block=ids.len();
  logits=forward(&ids,&mut cache);
 }
 println!("trace {} output tokens={}, stats={:?}",mode,generated.len(),model.streaming_stats());
}
