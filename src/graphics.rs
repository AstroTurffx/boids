use crate::bind_group::{create_bind_group, CompactBindGroupDescriptor, CompactBindGroupEntry};
use crate::camera::{Camera, CameraUniform};
use crate::camera_controller::CameraController;
use crate::texture::Texture;
use egui::{FontDefinitions, SidePanel};
use egui_wgpu_backend::{RenderPass, ScreenDescriptor};
use egui_winit_platform::{Platform, PlatformDescriptor};
use fps_counter::FPSCounter;
use instant::Instant;
use log::{debug, trace};
use std::time::Duration;
use wgpu::util::DeviceExt;
use wgpu::{Device, Queue, Surface, SurfaceConfiguration, TextureFormat};
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::window::Window;

pub(crate) struct State {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    window: Window,

    egui_platform: Platform,
    egui_render_pass: RenderPass,

    camera: Camera,
    camera_uniform: CameraUniform,
    camera_controller: CameraController,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    camera_buffer: wgpu::Buffer,

    texture_bind_group: wgpu::BindGroup,
    camera_bind_group: wgpu::BindGroup,

    render_pipeline: wgpu::RenderPipeline,

    num_indices: u32,
    fps: FPSCounter,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;

        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[rustfmt::skip]
const VERTICES: &[Vertex] = &[
    Vertex { position: [-0.0868241, 0.49240386, 0.0], tex_coords: [0.4131759, 0.00759614], },
    Vertex { position: [-0.49513406, 0.06958647, 0.0], tex_coords: [0.0048659444, 0.43041354], },
    Vertex { position: [-0.21918549, -0.44939706, 0.0], tex_coords: [0.28081453, 0.949397], },
    Vertex { position: [0.35966998, -0.3473291, 0.0], tex_coords: [0.85967, 0.84732914], },
    Vertex { position: [0.44147372, 0.2347359, 0.0], tex_coords: [0.9414737, 0.2652641], },
];

const INDICES: &[u16] = &[0, 1, 4, 1, 2, 4, 2, 3, 4];

impl State {
    // Creating some of the wgpu types requires async code
    pub(crate) async fn new(window: Window) -> Self {
        // --- Init ---
        trace!("Starting graphics state creation");
        let timer = Instant::now();
        let size = window.inner_size();

        let (device, queue, config, surface, format) = configure_surface(&window, size).await;

        // --- UI ---
        let egui_platform = Platform::new(PlatformDescriptor {
            physical_width: size.width,
            physical_height: size.height,
            scale_factor: window.scale_factor(),
            font_definitions: FontDefinitions::default(),
            style: Default::default(),
        });
        let egui_render_pass = RenderPass::new(&device, format, 1);

        // --- Textures ---
        trace!("Loading images");
        let diffuse_bytes = include_bytes!("assets/dom.png");
        let diffuse_texture =
            Texture::from_bytes(&device, &queue, diffuse_bytes, "dom.png").unwrap();

        // --- Shaders ---
        trace!("Creating shader module");
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/fish.wgsl"));

        // --- Camera ---
        let camera = Camera {
            // position the camera one unit up and 2 units back
            // +z is out of the screen
            eye: (0.0, 1.0, 2.0).into(),
            // have it look at the origin
            target: (0.0, 0.0, 0.0).into(),
            // which way is "up"
            up: cgmath::Vector3::unit_y(),
            aspect: config.width as f32 / config.height as f32,
            fovy: 60.0,
            znear: 0.1,
            zfar: 100.0,
        };
        let camera_controller = CameraController::new();
        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera);

        // --- Buffers ---
        trace!("Creating buffers");
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex_buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("index_buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera_buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let num_indices = INDICES.len() as u32;

        // --- Bind Groups ---
        let (texture_bind_group, texture_bind_group_layout) =
            diffuse_texture.create_bind_group(&device, Some("texture_bind_group"));
        let (camera_bind_group, camera_bind_group_layout) = create_bind_group(
            &device,
            CompactBindGroupDescriptor {
                label: Some("camera_bind_group"),
                entries: &[CompactBindGroupEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    resource: camera_buffer.as_entire_binding(),
                    count: None,
                }],
            },
        );

        // --- Render Pipeline ---
        trace!("Initializing render pipeline");
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render_pipeline_layout"),
                bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
                push_constant_ranges: &[],
            });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render_pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        debug!(
            "Graphics state creation finished in {:.2?}",
            timer.elapsed()
        );
        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            camera_buffer,
            texture_bind_group,
            camera,
            num_indices,
            camera_uniform,
            camera_bind_group,
            camera_controller,
            fps: FPSCounter::new(),
            egui_platform,
            egui_render_pass,
        }
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            debug!("Resizing to {:?}", new_size);
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub(crate) fn ui_handle_event<T>(&mut self, event: &Event<T>) {
        self.egui_platform.handle_event(event)
    }

    pub(crate) fn input(&mut self, event: &WindowEvent) -> bool {
        self.camera_controller.process_events(event)
    }

    pub(crate) fn update(&mut self, delta_s: Duration) {
        let delta = delta_s.as_secs_f64();
        self.egui_platform.update_time(delta);
        // let ui = self.imgui.frame();
        //
        // {
        //     let window = ui.window("Boids");
        //     window
        //         .size([200.0, 100.0], Condition::FirstUseEver)
        //         .position([5.0, 5.0], Condition::FirstUseEver)
        //         .resizable(false)
        //         .build(|| {
        //             ui.text(format!("FPS: {}", self.fps.tick()));
        //             ui.text(format!("Render time: {:?}ms", delta_s.as_millis()));
        //         });
        // }

        self.camera_controller.update_camera(
            &mut self.camera,
            delta as f32,
            self.egui_platform.context(),
        );
        self.camera_uniform.update_view_proj(&self.camera);
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[self.camera_uniform]),
        );
    }

    pub(crate) fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut render_encoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("render_encoder"),
                });

        let mut render_pass = render_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        // Clear color
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
        render_pass.set_bind_group(1, &self.texture_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..self.num_indices, 0, 0..1);

        drop(render_pass);

        // let ui_view = output
        //     .texture
        //     .create_view(&wgpu::TextureViewDescriptor::default());

        self.egui_platform.begin_frame();

        SidePanel::left("menu")
            .resizable(false)
            .default_width(150.0)
            .show(&self.egui_platform.context(), |ui| ui.label("hi"));

        let full_output = self.egui_platform.end_frame(Some(&self.window));
        let paint_jobs = self.egui_platform.context().tessellate(full_output.shapes);

        let mut ui_encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ui_encoder"),
            });

        // Upload all resources for the GPU.
        let screen_descriptor = ScreenDescriptor {
            physical_width: self.config.width,
            physical_height: self.config.height,
            scale_factor: self.window.scale_factor() as f32,
        };

        let tdelta: egui::TexturesDelta = full_output.textures_delta;
        self.egui_render_pass
            .add_textures(&self.device, &self.queue, &tdelta)
            .expect("add texture ok");
        self.egui_render_pass.update_buffers(
            &self.device,
            &self.queue,
            &paint_jobs,
            &screen_descriptor,
        );

        // Record all render passes.
        self.egui_render_pass
            .execute(
                &mut ui_encoder,
                &view,
                &paint_jobs,
                &screen_descriptor,
                None,
            )
            .unwrap();

        // submit will accept anything that implements IntoIter
        self.queue
            .submit([render_encoder.finish(), ui_encoder.finish()]);
        output.present();

        Ok(())
    }

    pub fn window(&self) -> &Window {
        &self.window
    }
    // pub fn ui(&mut self) -> &mut Context {
    //     &mut self.imgui
    // }
    pub fn size(&self) -> &PhysicalSize<u32> {
        &self.size
    }
}

async fn configure_surface(
    window: &Window,
    size: PhysicalSize<u32>,
) -> (Device, Queue, SurfaceConfiguration, Surface, TextureFormat) {
    // The instance is a handle to our GPU
    // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        dx12_shader_compiler: Default::default(),
    });
    trace!("WGPU instance successfully created");

    let surface = unsafe { instance.create_surface(window) }.unwrap();
    trace!("Surface successfully created");

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .unwrap();

    trace!("Searching for graphics adapter...");
    let adapter_timer = Instant::now();
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::empty(),
                // WebGL doesn't support all of wgpu's features, so if
                // we're building for the web we'll have to disable some.
                limits: if cfg!(target_arch = "wasm32") {
                    wgpu::Limits::downlevel_webgl2_defaults()
                } else {
                    wgpu::Limits::default()
                },
                label: None,
            },
            None, // Trace path
        )
        .await
        .unwrap();
    trace!(
        "Found graphics adapter (took {:.2?})",
        adapter_timer.elapsed()
    );

    trace!("Configuring surface");
    let surface_caps = surface.get_capabilities(&adapter);
    // Shader code in this tutorial assumes an sRGB surface texture. Using a different
    // one will result all the colors coming out darker. If you want to support non
    // sRGB surfaces, you'll need to account for that when drawing to the frame.
    let surface_format = surface_caps
        .formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(surface_caps.formats[0]);

    let config = SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: size.width,
        height: size.height,
        present_mode: surface_caps.present_modes[0],
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    (device, queue, config, surface, surface_format)
}
