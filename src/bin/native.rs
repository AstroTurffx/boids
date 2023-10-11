fn main() {
    pollster::block_on(boids::run());
}
