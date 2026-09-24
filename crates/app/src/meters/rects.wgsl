// Solid rectangles, one instance each: corners in clip space and a colour.

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) colour: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) corner: u32,
    @location(0) rect: vec4<f32>,
    @location(1) colour: vec4<f32>,
) -> Out {
    let x = select(rect.x, rect.z, (corner & 1u) == 1u);
    let y = select(rect.y, rect.w, (corner & 2u) == 2u);
    var out: Out;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.colour = colour;
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    return in.colour;
}
