use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct EffectPass {
    pub shader: String,
    pub uniforms: HashMap<String, UniformValue>,
    pub data_id: Option<String>,
}

#[derive(Clone, Debug)]
pub enum UniformValue {
    Number(f32),
    Vector(Vec<f32>),
}
