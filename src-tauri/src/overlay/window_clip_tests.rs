use super::*;

fn region(x: i32, y: i32, width: i32, height: i32) -> CaptureRegion {
    CaptureRegion::new(x, y, width, height)
}

#[test]
fn frame_ring_stays_thin_and_does_not_expand_for_controls() {
    let controls = [region(86, 174, 18, 18), region(98, 192, 14, 14)];
    let geometry = build_clip_geometry(
        Some(region(100, 200, 800, 400)),
        None,
        Some(&controls),
        Some(&[9.0, 7.0]),
        1.0,
    );

    let frame = geometry.frame_ring.expect("frame geometry");
    assert_eq!(
        frame.outer,
        ClipRect {
            x: 100,
            y: 200,
            width: 800,
            height: 400
        }
    );
    assert_eq!(
        frame.inner,
        Some(ClipRect {
            x: 102,
            y: 202,
            width: 796,
            height: 396
        })
    );
    assert_eq!(
        geometry.solid_rects,
        vec![
            RoundedClipRect {
                rect: ClipRect {
                    x: 86,
                    y: 174,
                    width: 18,
                    height: 18
                },
                radius: 9
            },
            RoundedClipRect {
                rect: ClipRect {
                    x: 98,
                    y: 192,
                    width: 14,
                    height: 14
                },
                radius: 7
            },
        ]
    );
}

#[test]
fn geometry_scales_frame_and_control_bounds_independently() {
    let controls = [region(10, 20, 18, 18)];
    let geometry = build_clip_geometry(
        Some(region(100, 200, 800, 400)),
        Some(region(120, 620, 300, 60)),
        Some(&controls),
        Some(&[4.0]),
        1.5,
    );

    let frame = geometry.frame_ring.expect("frame geometry");
    assert_eq!(
        frame.outer,
        ClipRect {
            x: 150,
            y: 300,
            width: 1200,
            height: 600
        }
    );
    assert_eq!(
        geometry.solid_rects,
        vec![
            RoundedClipRect {
                rect: ClipRect {
                    x: 180,
                    y: 930,
                    width: 450,
                    height: 90
                },
                radius: 12
            },
            RoundedClipRect {
                rect: ClipRect {
                    x: 15,
                    y: 30,
                    width: 27,
                    height: 27
                },
                radius: 6
            },
        ]
    );
}

#[test]
fn no_visible_elements_produces_empty_geometry() {
    let geometry = build_clip_geometry(None, None, None, None, 1.0);
    assert!(geometry.is_empty());
    assert!(geometry.solid_rects.is_empty());
}
