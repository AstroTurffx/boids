use crate::bind_group::{create_bind_group, CompactBindGroupDescriptor, CompactBindGroupEntry};
use crate::boids::{Boids, NUM_INSTANCES};
use crate::camera::{Camera, CameraUniform};
use crate::camera_controller::CameraController;
use crate::instance::InstanceRaw;
use crate::mipmaps::generate_mipmaps;
use crate::model::{DrawModel, Model, Vertex};
use crate::resources::load_model;
use crate::texture::Texture;
use egui::{
    Align, CentralPanel, Color32, FontDefinitions, Frame, Layout, Margin, Slider, TopBottomPanel,
};
use egui_wgpu_backend::{RenderPass, ScreenDescriptor};
use egui_winit_platform::{Platform, PlatformDescriptor};
use fps_counter::FPSCounter;
use instant::Instant;
use log::{debug, trace};
use std::time::Duration;
use wgpu::util::DeviceExt;
use wgpu::{
    BindGroupLayoutDescriptor, Device, Queue, Surface, SurfaceConfiguration, TextureFormat,
};
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::window::Window;

pub(crate) struct State {
    surface: Surface,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    size: PhysicalSize<u32>,
    window: Window,

    egui_platform: Platform,
    egui_render_pass: RenderPass,

    camera: Camera,
    camera_uniform: CameraUniform,
    camera_controller: CameraController,

    fish_model: Model,
    aquarium_model: Model,

    boids: Boids,

    depth_texture: Texture,
    multisampled_framebuffer: Texture,

    camera_buffer: wgpu::Buffer,

    camera_bind_group: wgpu::BindGroup,

    fish_pipeline: wgpu::RenderPipeline,
    aquarium_pipeline: wgpu::RenderPipeline,

    fps: FPSCounter,
}
const MSAA_SAMPLE_COUNT: u32 = 4;

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
        trace!("Loading textures");
        let depth_texture =
            Texture::create_depth_texture(&device, &config, "depth_texture", MSAA_SAMPLE_COUNT);
        let multisampled_framebuffer =
            Texture::create_msfb_texture(&device, &config, "mssa_texture", MSAA_SAMPLE_COUNT);

        // let diffuse_texture = load_texture("dom.png", &device, &queue).await.unwrap();

        // --- Camera ---
        let camera = Camera {
            // position the camera one unit up and 2 units back
            // +z is out of the screen
            eye: (0.0, 10.0, 20.0).into(),
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
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera_buffer"),
            contents: bytemuck::cast_slice(&[camera_uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // --- Bind Groups ---
        let texture_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });

        let boids_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: Some("tints_bind_group_layout"),
        });

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

        // --- Load models ---
        let fish_model = load_model("fish.obj", &device, &queue, &texture_bind_group_layout)
            .await
            .unwrap();
        let aquarium_model =
            load_model("aquarium.obj", &device, &queue, &texture_bind_group_layout)
                .await
                .unwrap();

        let boids = Boids::new(&device, &boids_bind_group_layout);

        // --- Render Pipeline ---
        trace!("Initializing render pipeline");
        let fish_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fish_pipeline_layout"),
            bind_group_layouts: &[
                &camera_bind_group_layout,
                &texture_bind_group_layout,
                &boids_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });
        let aquarium_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("aquarium_pipeline_layout"),
                bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
                push_constant_ranges: &[],
            });

        let fish_pipeline = {
            let shader = wgpu::include_wgsl!("shaders/fish.wgsl");
            create_render_pipeline(
                &device,
                &fish_pipeline_layout,
                config.format,
                Some(Texture::DEPTH_FORMAT),
                &[Vertex::desc(), InstanceRaw::desc()],
                shader,
            )
        };
        let aquarium_pipeline = {
            let shader = wgpu::include_wgsl!("shaders/aquarium.wgsl");
            create_render_pipeline(
                &device,
                &aquarium_pipeline_layout,
                config.format,
                Some(Texture::DEPTH_FORMAT),
                &[Vertex::desc()],
                shader,
            )
        };

        debug!(
            "Graphics state creation finished in {:.2?}",
            timer.elapsed()
        );

        // === Generate mip maps ===

        trace!("Generating mip maps...");
        let timer = Instant::now();

        let textures: Vec<&Texture> = [&fish_model, &aquarium_model]
            .iter()
            .flat_map(|model| {
                model
                    .materials
                    .iter()
                    .map(|material| &material.diffuse_texture)
            })
            .collect();
        let command_buf = generate_mipmaps(&device, &texture_bind_group_layout, &textures);
        queue.submit([command_buf]);

        debug!("Mip maps generated in {:.2?}", timer.elapsed());

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            fish_pipeline,
            aquarium_pipeline,
            camera_buffer,
            fish_model,
            aquarium_model,
            camera,
            camera_uniform,
            camera_bind_group,
            camera_controller,
            fps: FPSCounter::new(),
            egui_platform,
            egui_render_pass,
            depth_texture,
            multisampled_framebuffer,
            boids,
        }
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            debug!("Resizing to {:?}", new_size);
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;

            self.depth_texture = Texture::create_depth_texture(
                &self.device,
                &self.config,
                "depth_texture",
                MSAA_SAMPLE_COUNT,
            );
            self.multisampled_framebuffer = Texture::create_msfb_texture(
                &self.device,
                &self.config,
                "mssa_texture",
                MSAA_SAMPLE_COUNT,
            );

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

        self.boids.update(&self.queue);

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
                view: &self.multisampled_framebuffer.view,
                resolve_target: Some(&view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: true,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_texture.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });

        render_pass.set_pipeline(&self.aquarium_pipeline);
        render_pass.draw_model_instanced(&self.aquarium_model, 0..1, &self.camera_bind_group);

        render_pass.set_vertex_buffer(1, self.boids.buffer.slice(..));
        render_pass.set_bind_group(2, &self.boids.bind_group, &[]);
        render_pass.set_pipeline(&self.fish_pipeline);
        render_pass.draw_model_instanced(
            &self.fish_model,
            0..NUM_INSTANCES as u32,
            &self.camera_bind_group,
        );

        drop(render_pass);

        self.egui_platform.begin_frame();
        let fps = self.fps.tick();

        // SidePanel::left("menu")
        //     .resizable(false)
        //     .default_width(150.0)
        //     .show(&self.egui_platform.context(), |ui| ui.label("hi"));

        let bottom_bar = Frame {
            fill: Color32::from_rgb(15, 15, 15),
            inner_margin: Margin {
                left: 10.0,
                right: 10.0,
                top: 10.0,
                bottom: 6.0,
            },
            ..Default::default()
        };

        CentralPanel::default()
            .frame(Frame::none())
            .show(&self.egui_platform.context(), |ui| {
                let fps_text = format!("{} fps", fps);
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| ui.label(fps_text));
            });

        TopBottomPanel::bottom("bottom-bar").frame(bottom_bar).show(
            &self.egui_platform.context(),
            |ui| {
                let mut x = 0f32;
                let slider = Slider::new(&mut x, 0.0..=100.0);
                ui.horizontal(|ui| {
                    ui.label("abc");
                    ui.add(slider);
                })
            },
        );

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
fn create_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    vertex_layouts: &[wgpu::VertexBufferLayout],
    shader: wgpu::ShaderModuleDescriptor,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(shader);

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: vertex_layouts,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState {
                    alpha: wgpu::BlendComponent::REPLACE,
                    color: wgpu::BlendComponent::REPLACE,
                }),
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
        depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: MSAA_SAMPLE_COUNT,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
    })
}
