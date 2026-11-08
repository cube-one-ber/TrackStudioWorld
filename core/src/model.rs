//! A model as the viewport, the thumbnails and drop-to-ground use it: triangle meshes in model
//! space, one part per material, each with its albedo texture.

#[derive(Debug, Clone, Default)]
pub struct Part {
    pub material: String,
    /// the albedo texture's name: `Tex/<name>.bntx.zs` in the romfs
    pub texture: Option<String>,
    /// alpha-tested: texels below `alpha_cutoff` are not drawn
    pub cutout: bool,
    pub alpha_cutoff: f32,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// V = 0 is the texture's first (top) row
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub parts: Vec<Part>,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Model {
    pub fn triangle_count(&self) -> usize {
        self.parts.iter().map(|p| p.indices.len() / 3).sum()
    }

    /// Set `min` and `max` from the vertices (zero for an empty model).
    pub fn update_bounds(&mut self) {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for v in self.parts.iter().flat_map(|p| &p.positions) {
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
        if min[0] > max[0] {
            (min, max) = ([0.0; 3], [0.0; 3]);
        }
        (self.min, self.max) = (min, max);
    }
}
