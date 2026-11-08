//! Readback and monitor-local frame cropping.
use super::CaptureError;
use crate::config::CaptureRegion;
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11DeviceContext, ID3D11Texture2D, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
    D3D11_TEXTURE2D_DESC,
};

/// Read pixel data from a D3D11 texture
pub(super) fn read_texture_pixels(
    d3d_context: &ID3D11DeviceContext,
    texture: &ID3D11Texture2D,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, CaptureError> {
    unsafe {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc);

        let resource = texture
            .cast()
            .map_err(|e| CaptureError::D3DError(format!("Failed to cast texture: {}", e)))?;

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        d3d_context
            .Map(Some(&resource), 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|e| CaptureError::D3DError(format!("Failed to map texture: {}", e)))?;

        let bytes_per_pixel = 4u32;
        let buffer_size = (width * height * bytes_per_pixel) as usize;
        let mut buffer = vec![0u8; buffer_size];

        let src_slice = std::slice::from_raw_parts(
            mapped.pData as *const u8,
            (height * mapped.RowPitch) as usize,
        );

        for row in 0..height {
            let dst_start = (row * width * bytes_per_pixel) as usize;
            let dst_end = dst_start + (width * bytes_per_pixel) as usize;
            let src_start = (row * mapped.RowPitch) as usize;
            let src_end = src_start + (width * bytes_per_pixel) as usize;
            buffer[dst_start..dst_end].copy_from_slice(&src_slice[src_start..src_end]);
        }

        d3d_context.Unmap(Some(&resource), 0);

        Ok(buffer)
    }
}

/// Crop a region from the full screen capture
pub(super) fn crop_region(
    full_buffer: &[u8],
    full_width: u32,
    full_height: u32,
    region: &CaptureRegion,
) -> Result<Vec<u8>, CaptureError> {
    let bytes_per_pixel = 4u32;

    let region_right = region.x as i64 + region.width as i64;
    let region_bottom = region.y as i64 + region.height as i64;

    if !region.is_valid()
        || full_buffer.len() != full_width as usize * full_height as usize * 4
        || region.x < 0
        || region.y < 0
        || region_right > full_width as i64
        || region_bottom > full_height as i64
    {
        return Err(CaptureError::InvalidRegion(format!(
            "Region {:?} is outside screen bounds ({}x{})",
            region, full_width, full_height
        )));
    }

    let cropped_size = region.width as usize * region.height as usize * 4;
    let mut cropped = vec![0u8; cropped_size];

    for row in 0..region.height {
        let src_y = (region.y + row) as u32;
        let src_x = region.x as u32;

        let dst_start = (row * region.width * bytes_per_pixel as i32) as usize;
        let dst_end = dst_start + (region.width * bytes_per_pixel as i32) as usize;

        let src_start = ((src_y * full_width + src_x) * bytes_per_pixel) as usize;
        let src_end = src_start + (region.width * bytes_per_pixel as i32) as usize;

        cropped[dst_start..dst_end].copy_from_slice(&full_buffer[src_start..src_end]);
    }

    Ok(cropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_refuses_outside_regions_overflow_and_incomplete_frames() {
        for region in [
            CaptureRegion::new(-1, 0, 2, 2),
            CaptureRegion::new(3, 0, 2, 2),
            CaptureRegion::new(i32::MAX, 0, i32::MAX, 2),
            CaptureRegion::new(0, 0, -1, 2),
        ] {
            assert!(crop_region(&[0; 64], 4, 4, &region).is_err());
        }
        assert!(crop_region(&[0; 16], 4, 4, &CaptureRegion::new(0, 0, 2, 2)).is_err());
    }

    #[test]
    fn test_crop_region() {
        let mut full_buffer = vec![0u8; 64]; // 4x4x4

        for row in 0..4 {
            for col in 0..4 {
                let idx = (row * 4 + col) * 4;
                full_buffer[idx] = (row * 4 + col) as u8;
            }
        }

        let region = CaptureRegion::new(1, 1, 2, 2);
        let cropped = crop_region(&full_buffer, 4, 4, &region).unwrap();

        assert_eq!(cropped.len(), 16);
        assert_eq!(cropped[0], 5);
        assert_eq!(cropped[4], 6);
        assert_eq!(cropped[8], 9);
        assert_eq!(cropped[12], 10);
    }
}
