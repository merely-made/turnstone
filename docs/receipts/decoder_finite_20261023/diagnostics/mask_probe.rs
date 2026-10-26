use burn::tensor::{Tensor,TensorData,Device,DeviceKind,DType,Bool,activation::softmax};
fn main(){
 let d=Device::wgpu(DeviceKind::DiscreteGpu(0));
 let ex=Tensor::<1>::from_data(TensorData::new(vec![f32::NEG_INFINITY,f32::MIN,-1000.],[3]),(&d,DType::F32)).exp().into_data().try_to_vec::<f32>().unwrap();println!("exp={ex:?}");
 for n in [831usize,835,846,852,960] {for fill in [f32::NEG_INFINITY,f32::MIN] {
  let count=32;let past=n-89;
  let scores=Tensor::<4>::from_data(TensorData::new((0..12*count*n).map(|x|((x%37) as f32-18.)/10.).collect::<Vec<_>>(),[1,12,count,n]),(&d,DType::F32));
  let mask=Tensor::<2,Bool>::from_data(TensorData::new((0..count).flat_map(|q|(0..n).map(move |k|k>past+q)).collect::<Vec<_>>(),[count,n]),&d).unsqueeze::<4>().expand([1,12,count,n]);
  let v=Tensor::<4>::ones([1,12,n,128],(&d,DType::F32));
  let row=softmax(scores.mask_fill(mask,fill),3).matmul(v).into_data().try_to_vec::<f32>().unwrap();
  println!("n={n} fill={fill} invalid={} maxerr={}",row.iter().filter(|v|!v.is_finite()).count(),row.iter().map(|v|(*v-1.).abs()).fold(0f32,f32::max));
 }}
}
