// The Waveform on the GPU. Its columns sit still in a ring texture, each
// block of them written once at its place in time; only the camera moves,
// gliding by fractions of a pixel. A block always holds the same columns
// (a power of two of them, so each is 1/8 to 1/4 px wide, or one column
// when columns are wider than that), so what a block shows never depends
// on where the camera is.
//
// Each pixel is box-filtered: eight intervals across it, and for each the
// union of the blocks within a window exactly one pixel wide, its exact
// vertical coverage, then the average. The support never changes with the
// sub-pixel phase, so a peak keeps its weight as it slides: the picture
// translates smoothly, without the swelling and shrinking a blend of
// neighbours gives, or the uneven steps of moving by whole pixels. (After
// Dennis Oberst's waveform renderer: fixed block ownership and a constant
// one-pixel footprint.)
//
// Texture: one texel per block across (a ring: block n is at n mod width);
// two rows per lane: (min, max, low, mid) and (high, 0, 0, 0), as sampled.
// Blocks are counted back from the newest (0, −1, −2, …), so the numbers
// stay small however long the app runs. Output is premultiplied.

struct Params {
    // Clip space: left, top, right, bottom.
    rect: vec4<f32>,
    // The area in framebuffer pixels: x, y, width, height.
    area: vec4<f32>,
    // Where the right edge is in time (in blocks after the newest's start),
    // a block's width in px, blocks stored, unused.
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

/// One lookup: the block under `x` (in framebuffer px), if there is one.
struct Look {
    found: bool,
    envelope: vec4<f32>,
    high: f32,
};

fn look_at(x: f32, lane: u32) -> Look {
    let at = p.view.x - (p.area.x + p.area.z - x) / p.view.y;
    let block = floor(at);
    var out: Look;
    out.found = block >= 1.0 - p.view.z && block <= 0.0;
    if out.found {
        let texels = fetch(block, lane);
        out.envelope = texels[0];
        out.high = texels[1].x;
    }
    return out;
}

/// Intervals across a pixel, and lookups spanning one pixel's window.
const TAPS: u32 = 8u;
const WINDOW: u32 = 9u;
const EDGES: u32 = 16u;

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    let px = in.position.xy;
    let lanes = p.ring.z;
    let lane_height = p.area.w / lanes;
    let lane = u32(clamp(floor((px.y - p.area.y) / lane_height), 0.0, lanes - 1.0));
    let centre = p.area.y + lane_height * (f32(lane) + 0.5);
    let half = lane_height * 0.5 - p.ring.w;
    let step = 1.0 / f32(TAPS);

    // Lookups every 1/8 px, from half a pixel left of the first interval's
    // centre to half a pixel right of the last one's.
    var edge: array<Look, EDGES>;
    for (var j = 0u; j < EDGES; j = j + 1u) {
        edge[j] = look_at(px.x - 1.0 + 0.5 * step + f32(j) * step, lane);
    }

    let mode = u32(p.look.z);
    var ink = vec4<f32>(0.0);
    for (var k = 0u; k < TAPS; k = k + 1u) {
        // The union of the blocks in this interval's one-pixel window.
        var found = false;
        var lo = 1e9;
        var hi = -1e9;
        var bands = vec3<f32>(0.0);
        for (var j = 0u; j < WINDOW; j = j + 1u) {
            let l = edge[k + j];
            if l.found {
                found = true;
                lo = min(lo, l.envelope.x);
                hi = max(hi, l.envelope.y);
                bands = max(bands, vec3<f32>(l.envelope.z, l.envelope.w, l.high));
            }
        }
        if !found {
            continue;
        }
        let total = bands.x + bands.y + bands.z;
        var colour = vec3<f32>(0.0);
        var alpha = 0.0;
        if mode == 2u {
            // Rekordbox: each band as high as its share of the peak, low
            // widest underneath, high on top.
            if total > 0.0 {
                let peak = max(hi, -lo);
                let colours = array<vec3<f32>, 3>(p.low.rgb, p.mid.rgb, p.high.rgb);
                for (var i = 0u; i < 3u; i = i + 1u) {
                    let level = height(peak * sqrt(bands[i] / total)) * half;
                    let c = cover(px.y, centre - level, centre + level);
                    colour = colours[i] * c + colour * (1.0 - c);
                    alpha = c + alpha * (1.0 - c);
                }
                if alpha > 0.0 {
                    colour = colour / alpha;
                }
            }
        } else {
            alpha = cover(px.y, centre - height(hi) * half, centre - height(lo) * half);
            colour = p.mid.rgb;
            if mode == 1u && total > 0.0 {
                // The band colours mixed by energy, as bright as the brightest.
                let mixed = (p.low.rgb * bands.x + p.mid.rgb * bands.y + p.high.rgb * bands.z)
                    / total;
                let brightest = max(max(mixed.r, mixed.g), max(mixed.b, 1.0 / 255.0));
                colour = min(mixed * (p.ceiling.x / brightest), vec3<f32>(1.0));
            }
        }
        ink += vec4<f32>(to_linear(colour) * alpha, alpha);
    }
    ink = ink / f32(TAPS);
    if ink.a < 0.002 {
        discard;
    }
    return ink;
}
