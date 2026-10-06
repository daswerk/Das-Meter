// A heat map (the Spectrogram): levels in a texture, one texture row per
// column of time (oldest first), one texel per frequency row, coloured
// through a palette of 256 colours. The picture scrolls by `lag` columns, so
// it moves smoothly between columns; each pixel takes the loudest of a few
// samples across what it covers, so peaks survive when there are more
// columns or rows than pixels.

struct Params {
    // Clip space: left, top, right, bottom.
    rect: vec4<f32>,
    // Columns stored, columns across the plot, columns still to scroll in, rows.
    info: vec4<f32>,
    // Columns and rows per pixel.
    footprint: vec4<f32>,
    // Under the picture where its alpha is 1, else none.
    back: vec4<f32>,
    palette: array<vec4<f32>, 256>,
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var levels: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    // 0..1 across (left to right) and up (bottom to top).
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) corner: u32) -> Out {
    let right = (corner & 1u) == 1u;
    let bottom = (corner & 2u) == 2u;
    var out: Out;
    out.position = vec4<f32>(
        select(p.rect.x, p.rect.z, right),
        select(p.rect.y, p.rect.w, bottom),
        0.0,
        1.0,
    );
    out.uv = vec2<f32>(select(0.0, 1.0, right), select(1.0, 0.0, bottom));
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    let stored = p.info.x;
    let across = p.info.y;
    let lag = p.info.z;
    let rows = p.info.w;
    // Columns back from the newest; texel space along time.
    let column = stored - ((1.0 - in.uv.x) * across + lag);
    if column < 0.0 {
        if p.back.a > 0.0 {
            return p.back;
        }
        discard;
    }
    let row = in.uv.y * rows;
    let per_column = max(p.footprint.x, 1.0);
    let per_row = max(p.footprint.y, 1.0);
    let taps_x = u32(clamp(ceil(per_column), 1.0, 4.0));
    let taps_y = u32(clamp(ceil(per_row), 1.0, 4.0));
    var level = 0.0;
    for (var i = 0u; i < taps_x; i = i + 1u) {
        let c = column - per_column * 0.5 + per_column * (f32(i) + 0.5) / f32(taps_x);
        let y = clamp(c, 0.5, max(stored - 0.5, 0.5)) / across;
        for (var j = 0u; j < taps_y; j = j + 1u) {
            let r = row - per_row * 0.5 + per_row * (f32(j) + 0.5) / f32(taps_y);
            let x = clamp(r, 0.5, rows - 0.5) / rows;
            level = max(level, textureSampleLevel(levels, linear_sampler, vec2<f32>(x, y), 0.0).r);
        }
    }
    let colour = p.palette[u32(round(clamp(level, 0.0, 1.0) * 255.0))];
    if p.back.a > 0.0 {
        return vec4<f32>(mix(p.back.rgb, colour.rgb, colour.a), 1.0);
    }
    return colour;
}
