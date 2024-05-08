pub fn max3(l1: f32, l2: f32, l3: f32) -> f32 {
    if l1 > l2 {
        if l1 > l3 {
            l1
        } else {
            l3
        }
    } else if l2 > l3 {
        l2
    } else {
        l3
    }
}

pub fn min3(l1: f32, l2: f32, l3: f32) -> f32 {
    if l1 < l2 {
        if l1 < l3 {
            l1
        } else {
            l3
        }
    } else if l2 < l3 {
        l2
    } else {
        l3
    }
}

pub fn get_current_timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs_f64()
}


use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use bevy::render::texture::Image;
/// Creates a colorful test pattern
#[allow(dead_code)]
pub fn uv_debug_texture() -> Image {
    const TEXTURE_SIZE: usize = 8;

    let mut palette: [u8; 32] = [
        255, 102, 159, 255, 255, 159, 102, 255, 236, 255, 102, 255, 121, 255,
        102, 255, 102, 255, 198, 255, 102, 198, 255, 255, 121, 102, 255, 255,
        236, 102, 255, 255,
    ];

    let mut texture_data = [0; TEXTURE_SIZE * TEXTURE_SIZE * 4];
    for y in 0..TEXTURE_SIZE {
        let offset = TEXTURE_SIZE * y * 4;
        texture_data[offset..(offset + TEXTURE_SIZE * 4)]
            .copy_from_slice(&palette);
        palette.rotate_right(4);
    }
    Image::new_fill(
        Extent3d {
            width: TEXTURE_SIZE as u32,
            height: TEXTURE_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &texture_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}
