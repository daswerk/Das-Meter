// The Waveform on the GPU. Its columns sit still in a ring texture, each
// group of them (about a pixel's worth) written once at its place in time;
// only the camera moves, by whole pixels. Every pixel asks where it is in
// time (the camera, less its distance from the right edge) and shows the
// one group there, as it is. So the picture only ever shifts: a column
// looks the same in every frame (no blending between neighbours, which
// made peaks swell and shrink as they slid), so nothing can flicker.
//
// Texture: one texel per group across (a ring: group n is at n mod width);
// two rows per lane: (min, max, low, mid) and (high, 0, 0, 0), as sampled.
// Groups are counted back from the newest (0, −1, −2, …), so the numbers
// stay small however long the app runs.

struct Params {
    // Clip space: left, top, right, bottom.
    rect: vec4<f32>,
    // The area in framebuffer pixels: x, y, width, height.
    area: vec4<f32>,
    // Where the right edge is in time (in groups after the newest's start),
    // a group's width in px, groups stored, unused.
    view: vec4<f32>,
    // Ring width, the newest group's place in the ring, lanes, the lane's
    // margin in px.
    ring: vec4<f32>,
    // Gain, 1 for the dB scale, colouring (0 solid, 1 RGB, 2 Rekordbox),
    // 1 to write linear light (an sRGB target).
    look: vec4<f32>,
    // The band colours (sRGB, 0..1) and the brightest channel among them.
    low: vec4<f32>,
    mid: vec4<f32>,
    high: vec4<f32>,
    ceiling: vec4<f32>,
};

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var groups: texture_2d<f32>;

struct Out {
    @builtin(position) position: vec4<f32>,
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
    return out;
}

// One group's two texels for a lane, by how far back from the newest it is.
fn fetch(back: f32, lane: u32) -> array<vec4<f32>, 2> {
    let width = i32(p.ring.x);
    let x = u32(((i32(p.ring.y) + i32(back)) % width + width) % width);
    return array<vec4<f32>, 2>(
        textureLoad(groups, vec2<u32>(x, lane * 2u), 0),
        textureLoad(groups, vec2<u32>(x, lane * 2u + 1u), 0),
    );
}

// A sample to its height in −1..1: gain, then the linear or dB scale.
fn height(sample: f32) -> f32 {
    let x = clamp(sample * p.look.x, -1.0, 1.0);
    if p.look.y < 0.5 {
        return x;
    }
    let magnitude = abs(x);
    if magnitude <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * log2(magnitude) / log2(10.0);
    return sign(x) * clamp((db + 60.0) / 60.0, 0.0, 1.0);
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    if p.look.w < 0.5 {
        return c;
    }
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

// How much of this pixel the span from `top` to `bottom` (in px) covers,
// at least a pixel high so silence still shows a line.
fn cover(y: f32, top: f32, bottom: f32) -> f32 {
    let centre = (top + bottom) * 0.5;
    let half = max(bottom - top, 1.0) * 0.5;
    return clamp(half + 0.5 - abs(y - centre), 0.0, 1.0);
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    let px = in.position.xy;
    let lanes = p.ring.z;
    let lane_height = p.area.w / lanes;
    let lane = u32(clamp(floor((px.y - p.area.y) / lane_height), 0.0, lanes - 1.0));
    let centre = p.area.y + lane_height * (f32(lane) + 0.5);
    let half = lane_height * 0.5 - p.ring.w;

    // Where this pixel is in time, in groups: the camera, less how far it
    // is from the right edge. Group n spans n..n+1.
    let at = p.view.x - (p.area.x + p.area.z - px.x) / p.view.y;
    let oldest = 1.0 - p.view.z;
    let group = floor(at);
    if group < oldest {
        discard;
    }
    let texels = fetch(min(group, 0.0), lane);
    let envelope = texels[0];
    let lo = envelope.z;
    let md = envelope.w;
    let hi = texels[1].x;

    var colour = vec3<f32>(0.0);
    var alpha = 0.0;
    let mode = u32(p.look.z);
    if mode == 2u {
        // Rekordbox: each band as high as its share of the peak, low widest
        // underneath, high on top.
        let peak = max(envelope.y, -envelope.x);
        let total = lo + md + hi;
        if total > 0.0 {
            let levels = array<f32, 3>(lo, md, hi);
            let colours = array<vec3<f32>, 3>(p.low.rgb, p.mid.rgb, p.high.rgb);
            for (var i = 0u; i < 3u; i = i + 1u) {
                let level = height(peak * sqrt(levels[i] / total)) * half;
                let c = cover(px.y, centre - level, centre + level);
                colour = mix(colour, colours[i], c);
                alpha = max(alpha, c);
            }
        }
    } else {
        let top = centre - height(envelope.y) * half;
        let bottom = centre - height(envelope.x) * half;
        alpha = cover(px.y, top, bottom);
        colour = p.mid.rgb;
        let total = lo + md + hi;
        if mode == 1u && total > 0.0 {
            // The band colours mixed by energy, as bright as the brightest.
            let mixed = (p.low.rgb * lo + p.mid.rgb * md + p.high.rgb * hi) / total;
            let brightest = max(max(mixed.r, mixed.g), max(mixed.b, 1.0 / 255.0));
            colour = min(mixed * (p.ceiling.x / brightest), vec3<f32>(1.0));
        }
    }
    if alpha <= 0.0 {
        discard;
    }
    return vec4<f32>(to_linear(colour), alpha);
}
