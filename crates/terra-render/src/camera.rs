use glam::{Mat4, Vec3};

#[derive(Debug, Clone)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 1500.0,
            yaw: 0.7,
            pitch: 0.6,
            fov_y: 50f32.to_radians(),
            near: 1.0,
            far: 100_000.0,
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        let offset =
            Vec3::new(self.yaw.cos() * cp, self.pitch.sin(), self.yaw.sin() * cp) * self.distance;
        self.target + offset
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(self.fov_y, aspect.max(0.01), self.near, self.far);
        proj * view
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * 0.005;
        self.pitch = (self.pitch + dy * 0.005).clamp(-1.45, 1.45);
    }

    pub fn zoom(&mut self, delta: f32) {
        // Exponential zoom so wheel steps feel even near and far.
        let factor = (1.0 - delta * 0.0015).clamp(0.5, 1.5);
        self.distance = (self.distance * factor).clamp(10.0, 50_000.0);
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let eye = self.eye();
        let forward = (self.target - eye).normalize_or_zero();
        // Move the look-at across the world XZ plane (camera flies over fixed terrain).
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let mut flat_forward = Vec3::new(forward.x, 0.0, forward.z);
        if flat_forward.length_squared() < 1e-6 {
            flat_forward = Vec3::new(-self.yaw.sin(), 0.0, self.yaw.cos());
        } else {
            flat_forward = flat_forward.normalize();
        }
        let scale = self.distance * 0.00125;
        // Screen-Y is inverted vs world forward: drag up → pan look-at forward.
        self.target += right * (-dx * scale) + flat_forward * (dy * scale);
    }

    /// Keep the orbit target over the heightfield footprint.
    pub fn clamp_to_world(&mut self, world_size: (f32, f32)) {
        let max_x = world_size.0.max(1.0);
        let max_z = world_size.1.max(1.0);
        self.target.x = self.target.x.clamp(0.0, max_x);
        self.target.z = self.target.z.clamp(0.0, max_z);
    }

    pub fn reset(&mut self, target: Vec3, distance: f32) {
        self.target = target;
        self.distance = distance;
        self.yaw = 0.7;
        self.pitch = 0.6;
    }
}
