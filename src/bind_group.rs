use std::num::NonZeroU32;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, Device, Label,
};

pub struct CompactBindGroupDescriptor<'a> {
    pub label: Label<'a>,
    pub entries: &'a [CompactBindGroupEntry<'a>],
}

pub struct CompactBindGroupEntry<'a> {
    pub binding: u32,
    pub visibility: wgpu::ShaderStages,
    pub ty: wgpu::BindingType,
    pub count: Option<NonZeroU32>,
    pub resource: BindingResource<'a>,
}

impl<'a> CompactBindGroupDescriptor<'a> {
    fn to_bind_group_layout_entry(&self) -> Vec<BindGroupLayoutEntry> {
        self.entries
            .iter()
            .map(|x| BindGroupLayoutEntry {
                binding: x.binding,
                visibility: x.visibility,
                ty: x.ty,
                count: x.count,
            })
            .collect::<Vec<BindGroupLayoutEntry>>()
    }
    fn to_bind_group_entry(&self) -> Vec<BindGroupEntry> {
        self.entries
            .iter()
            .map(|x| BindGroupEntry {
                binding: x.binding,
                resource: x.resource.clone(),
            })
            .collect::<Vec<BindGroupEntry>>()
    }
}

pub(crate) fn create_bind_group(
    device: &Device,
    desc: CompactBindGroupDescriptor,
) -> (wgpu::BindGroup, BindGroupLayout) {
    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        entries: desc.to_bind_group_layout_entry().as_ref(),
        label: desc.label.map(|x| format!("{}_layout", x)).as_deref(),
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        layout: &bind_group_layout,
        entries: desc.to_bind_group_entry().as_ref(),
        label: desc.label,
    });
    (bind_group, bind_group_layout)
}
