use burn::tensor::{Tensor,TensorData,Device,DeviceKind,DType};
fn main(){let d=Device::wgpu(DeviceKind::DiscreteGpu(0));let n=151936;let h=1536;
 let w=Tensor::<2>::ones([n,h],(&d,DType::F32)).transpose();
 for poison in [0f32,f32::NAN,f32::NEG_INFINITY] {for pass in 0..3 {
  {let p=Tensor::<3>::full([1,1,n],poison,(&d,DType::F32));let _=p.into_data();}
  let x=Tensor::<3>::from_data(TensorData::new(vec![1f32/h as f32;h],[1,1,h]),(&d,DType::F32));
  let a=x.matmul(w.clone().unsqueeze::<3>());
  let row=a.slice([0..1,0..1,0..n]).into_data().try_to_vec::<f32>().unwrap();println!("poison={poison} pass={pass} nonfinite={} maxerr={}",row.iter().filter(|v|!v.is_finite()).count(),row.iter().map(|v|(*v-1.).abs()).fold(0f32,f32::max));
 }}
}
