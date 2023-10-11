use crate::camera::Camera;
use cgmath::num_traits::Pow;
use imgui::{Condition, MouseButton, Ui};
use imgui_winit_support::winit::event::{MouseScrollDelta, WindowEvent};
use log::debug;

pub(crate) struct CameraController {
    auto_rotate: bool,
    scroll_speed: f32,
    scroll_delta: f32,
    drag_speed: f32,
    last_drag: f32,
}

impl CameraController {
    pub(crate) fn new() -> Self {
        Self {
            auto_rotate: false,
            scroll_speed: 6.0,
            scroll_delta: 0.0,
            drag_speed: 0.025,
            last_drag: 0.0,
        }
    }

    pub(crate) fn process_events(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::MouseWheel { delta, .. } => {
                self.scroll_delta += match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => (p.y as f32) / 100.0f32,
                };
                true
            }

            _ => false,
        }
    }

    pub(crate) fn update_camera(&mut self, camera: &mut Camera, delta: f32, ui: &mut Ui) {
        use cgmath::InnerSpace;
        let speed = self.scroll_speed * delta;
        let forward = camera.target - camera.eye;
        let forward_norm = forward.normalize();
        let forward_mag = forward.magnitude();

        // Prevents glitching when camera gets too close to the
        // center of the scene.
        // I'm not using the normalized forward vector as I want
        // the zoom to depend on the distance to the target.
        if self.scroll_delta > 0.0 && forward_mag > speed {
            camera.eye += forward * delta * self.scroll_delta * self.scroll_speed;
        }
        if self.scroll_delta < 0.0 {
            camera.eye += forward * delta * self.scroll_delta * self.scroll_speed
        }

        let mut delta: f32 = 0.0;

        if ui.is_mouse_dragging(MouseButton::Middle) {
            let drag = ui.mouse_drag_delta_with_button(MouseButton::Middle)[0];
            delta = drag - self.last_drag;
            self.last_drag = drag;
        } else {
            self.last_drag = 0.0
        }

        if self.auto_rotate {
            delta += 0.2;
        }

        if delta != 0.0 {
            let right = forward_norm.cross(camera.up);

            // Redo radius calc in case the forward/backward is pressed.
            let forward = camera.target - camera.eye;
            let forward_mag = forward.magnitude();

            camera.eye = camera.target
                - (forward + right * self.drag_speed * delta).normalize() * forward_mag;
        }

        self.scroll_delta = 0.0;
        // Update UI
        {
            let window = ui.window("Camera");
            window
                .size([200.0, 100.0], Condition::FirstUseEver)
                .position([210.0, 5.0], Condition::FirstUseEver)
                .resizable(false)
                .build(|| {
                    ui.checkbox("Auto-rotate", &mut self.auto_rotate);
                    ui.separator();
                    ui.text(format!("Scroll speed: {}", self.scroll_speed));
                    ui.text(format!("Drag speed: {}", self.drag_speed));
                });
        }
    }
}
