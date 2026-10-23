use burn::tensor::{Tensor,TensorData,Device,DType};
fn main(){let d=Device::wgpu(burn::tensor::DeviceKind::DiscreteGpu(0));let a=Tensor::<1>::from_data(TensorData::new(vec![1f32],[1]),&d);let b=Tensor::<1>::from_data(TensorData::new(vec![1f32],[1]),(&d,DType::F32));println!("default={:?} explicit={:?}",a.dtype(),b.dtype());}
