use crate::instance::Instance;
use cgmath::*;
use rand::distributions::{Distribution, Standard};
use rand::Rng;
use std::mem::MaybeUninit;
use std::ops::Range;
use wgpu::util::DeviceExt;
use wgpu::{BindGroupLayout, Buffer, BufferUsages, Device, Queue};

const AQUARIUM_RADIUS: f32 = 20.0;
const AQUARIUM_SIZE: Range<f32> = -AQUARIUM_RADIUS..AQUARIUM_RADIUS;
pub const NUM_INSTANCES: usize = 50;

pub struct Boids {
    pub instances: [Instance; NUM_INSTANCES],
    pub buffer: Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl Boids {
    pub fn new(device: &Device, layout: &BindGroupLayout) -> Self {
        let mut rng = rand::thread_rng();

        let instances = unsafe {
            let mut array = MaybeUninit::<[Instance; NUM_INSTANCES]>::uninit();
            for x in array.assume_init_mut() {
                *x = rng.gen();
            }
            array.assume_init()
        };

        let tints = unsafe {
            let mut array = MaybeUninit::<[[f32; 3]; NUM_INSTANCES]>::uninit();
            for x in array.assume_init_mut() {
                *x = [rng.gen(), rng.gen(), rng.gen()]
            }
            array.assume_init()
        };

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tint_buffer"),
            contents: bytemuck::cast_slice(&tints),
            usage: BufferUsages::STORAGE,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tints_bind_group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        let raw_data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("instance_buffer"),
            contents: bytemuck::cast_slice(&raw_data),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });

        Self {
            instances,
            buffer,
            bind_group,
        }
    }

    pub fn update(&mut self, queue: &Queue) {
        // Run boids simulation
        //
        // TODO: Octree search

        // Write data to buffer
        let raw_data = self
            .instances
            .iter()
            .map(Instance::to_raw)
            .collect::<Vec<_>>();
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&raw_data));
    }
}

impl Distribution<Instance> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Instance {
        let position = Vector3 {
            x: rng.gen_range(AQUARIUM_SIZE),
            y: rng.gen_range(AQUARIUM_SIZE),
            z: rng.gen_range(AQUARIUM_SIZE),
        };

        let rotation = Quaternion::from_axis_angle(Vector3::unit_z(), Deg(0.0));

        Instance { position, rotation }
    }
}
