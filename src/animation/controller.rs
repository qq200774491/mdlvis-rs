// Controller data handling
// Based on GetFrameData function from mdlDraw.pas

use super::types::*;
use crate::animation::interpolation::{
    bezier, hermite, linear, nested_slerp_quaternion, slerp_quaternion,
};
use crate::error::MdlError;
use crate::model::ids::{GlobalSeqId, TrackId};
use crate::model::model::Model;
use crate::model::skeleton::{AnimationController, Keyframe};
use crate::model::tracks::InterpolationType;

/// Get interpolated frame data from controller
/// Based on GetFrameData function in mdlDraw.pas (lines 776-960)
pub fn get_frame_data(controllers: &[Controller], controller_idx: i32, frame: i32) -> Vec<f32> {
    if controller_idx < 0 || controller_idx as usize >= controllers.len() {
        return vec![0.0; 4]; // Default values
    }

    let controller = &controllers[controller_idx as usize];
    controller.get_frame_data(frame)
}

/// Sample a scalar controller without mutating model or playback state.
#[allow(dead_code)]
pub fn sample_scalar(
    model: &Model,
    track: TrackId,
    frame: &ResolvedFrame,
    default: f32,
) -> Result<f32, MdlError> {
    let Some(sample) = sample_track::<1>(model, track, frame)? else {
        return Ok(default);
    };
    Ok(sample[0])
}

/// Sample a three-component controller without mutating model or playback state.
#[allow(dead_code)]
pub fn sample_vec3(
    model: &Model,
    track: TrackId,
    frame: &ResolvedFrame,
    default: [f32; 3],
) -> Result<[f32; 3], MdlError> {
    Ok(sample_track::<3>(model, track, frame)?.unwrap_or(default))
}

/// Sample a quaternion controller using the original shortest-path rotation formulas.
#[allow(dead_code)]
pub fn sample_quaternion(
    model: &Model,
    track: TrackId,
    frame: &ResolvedFrame,
    default: [f32; 4],
) -> Result<[f32; 4], MdlError> {
    let Some(controller) = get_controller(model, track)? else {
        return Ok(default);
    };
    if controller.keyframes.is_empty() {
        return Ok(default);
    }
    let interpolation = validate_controller::<4>(controller, true)?;
    let Some(selection) = select_keys(model, controller, frame)? else {
        return Ok(default);
    };
    let result = match selection {
        KeySelection::Exact(key) | KeySelection::Single(key) => array_from_slice(&key.data),
        KeySelection::Between { before, after, t } => match interpolation {
            InterpolationType::None => array_from_slice(&before.data),
            InterpolationType::Linear => slerp_quaternion(
                array_from_slice(&before.data),
                array_from_slice(&after.data),
                t,
                0.001,
            ),
            InterpolationType::Hermite | InterpolationType::Bezier => nested_slerp_quaternion(
                array_from_slice(&before.data),
                array_from_slice(&before.out_tan),
                array_from_slice(&after.data),
                array_from_slice(&after.in_tan),
                t,
            ),
        },
    };
    validate_finite(&result)?;
    Ok(result)
}

/// Sample a scalar track and round it using ties-to-even integer semantics.
#[allow(dead_code)]
pub fn sample_discrete(
    model: &Model,
    track: TrackId,
    frame: &ResolvedFrame,
    default: i32,
) -> Result<i32, MdlError> {
    let Some(sample) = sample_track::<1>(model, track, frame)? else {
        return Ok(default);
    };
    let value = sample[0];
    if !value.is_finite() {
        return Err(MdlError::new("animation-non-finite-track-value").with_arg("value", value));
    }
    let rounded = f64::from(value).round_ties_even();
    if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return Err(MdlError::new("animation-discrete-value-out-of-range").with_arg("value", value));
    }
    Ok(rounded as i32)
}

fn sample_track<const N: usize>(
    model: &Model,
    track: TrackId,
    frame: &ResolvedFrame,
) -> Result<Option<[f32; N]>, MdlError> {
    let Some(controller) = get_controller(model, track)? else {
        return Ok(None);
    };
    if controller.keyframes.is_empty() {
        return Ok(None);
    }
    let interpolation = validate_controller::<N>(controller, false)?;
    let Some(selection) = select_keys(model, controller, frame)? else {
        return Ok(None);
    };
    let result = match selection {
        KeySelection::Exact(key) | KeySelection::Single(key) => array_from_slice(&key.data),
        KeySelection::Between { before, after, t } => match interpolation {
            InterpolationType::None => array_from_slice(&before.data),
            InterpolationType::Linear => linear(
                array_from_slice(&before.data),
                array_from_slice(&after.data),
                t,
            ),
            InterpolationType::Hermite => hermite(
                array_from_slice(&before.data),
                array_from_slice(&before.out_tan),
                array_from_slice(&after.data),
                array_from_slice(&after.in_tan),
                t,
            ),
            InterpolationType::Bezier => bezier(
                array_from_slice(&before.data),
                array_from_slice(&before.out_tan),
                array_from_slice(&after.data),
                array_from_slice(&after.in_tan),
                t,
            ),
        },
    };
    validate_finite(&result)?;
    Ok(Some(result))
}

fn get_controller(model: &Model, track: TrackId) -> Result<Option<&AnimationController>, MdlError> {
    if track.is_none() {
        return Ok(None);
    }
    model
        .controllers
        .get(track.0 as usize)
        .map(Some)
        .ok_or_else(|| {
            MdlError::new("animation-invalid-controller-index")
                .with_arg("index", track.0)
                .with_arg("count", model.controllers.len())
        })
}

fn validate_controller<const N: usize>(
    controller: &AnimationController,
    quaternion: bool,
) -> Result<InterpolationType, MdlError> {
    let interpolation =
        InterpolationType::from_u32(controller.interpolation_type).ok_or_else(|| {
            MdlError::new("animation-invalid-interpolation-type")
                .with_arg("value", controller.interpolation_type)
        })?;
    for pair in controller.keyframes.windows(2) {
        if pair[0].frame >= pair[1].frame {
            return Err(MdlError::new("animation-invalid-keyframe-order")
                .with_arg("previous", pair[0].frame)
                .with_arg("next", pair[1].frame));
        }
    }
    for key in &controller.keyframes {
        if key.data.len() != N {
            return Err(MdlError::new("animation-invalid-track-width")
                .with_arg("expected", N)
                .with_arg("actual", key.data.len())
                .with_arg("frame", key.frame));
        }
        if interpolation.has_tangents() && (key.in_tan.len() != N || key.out_tan.len() != N) {
            return Err(MdlError::new("animation-invalid-tangent-width")
                .with_arg("expected", N)
                .with_arg("in_actual", key.in_tan.len())
                .with_arg("out_actual", key.out_tan.len())
                .with_arg("frame", key.frame));
        }
        if quaternion {
            validate_quaternion(&key.data)?;
            if interpolation.has_tangents() {
                validate_quaternion(&key.in_tan)?;
                validate_quaternion(&key.out_tan)?;
            } else {
                validate_finite(&key.in_tan)?;
                validate_finite(&key.out_tan)?;
            }
        } else {
            validate_finite(&key.data)?;
            validate_finite(&key.in_tan)?;
            validate_finite(&key.out_tan)?;
        }
    }
    Ok(interpolation)
}

fn validate_finite(values: &[f32]) -> Result<(), MdlError> {
    if let Some(value) = values.iter().find(|value| !value.is_finite()) {
        Err(MdlError::new("animation-non-finite-track-value").with_arg("value", value))
    } else {
        Ok(())
    }
}

fn validate_quaternion(value: &[f32]) -> Result<(), MdlError> {
    let length_squared: f32 = value.iter().map(|component| component * component).sum();
    if length_squared.is_finite() && length_squared > 0.0 {
        Ok(())
    } else {
        Err(MdlError::new("animation-invalid-quaternion"))
    }
}

enum KeySelection<'a> {
    Exact(&'a Keyframe),
    Single(&'a Keyframe),
    Between {
        before: &'a Keyframe,
        after: &'a Keyframe,
        t: f32,
    },
}

fn select_keys<'a>(
    model: &Model,
    controller: &'a AnimationController,
    frame: &ResolvedFrame,
) -> Result<Option<KeySelection<'a>>, MdlError> {
    if controller.global_seq_id >= 0 {
        let global_id = GlobalSeqId(controller.global_seq_id);
        let resolved = super::types::resolve_global_frame(model, global_id, frame.global_frame)?;
        let duration = model.global_sequences[global_id.0 as usize].duration;
        if duration == 0 {
            return Ok(controller.keyframes.first().map(KeySelection::Single));
        }
        return select_in_domain(
            &controller.keyframes,
            resolved,
            0.0,
            f64::from(duration),
            PlaybackMode::Loop,
        );
    }

    if !frame.sequence_frame.is_finite() {
        return Err(
            MdlError::new("animation-invalid-frame-time").with_arg("time", frame.sequence_frame)
        );
    }
    let Some(sequence_index) = frame.sequence else {
        return Ok(select_unbounded(
            &controller.keyframes,
            frame.sequence_frame,
        ));
    };
    let sequence = model.sequences.get(sequence_index).ok_or_else(|| {
        MdlError::new("animation-invalid-sequence-index")
            .with_arg("index", sequence_index)
            .with_arg("count", model.sequences.len())
    })?;
    if sequence.start_frame > sequence.end_frame {
        return Err(MdlError::new("animation-invalid-sequence-range")
            .with_arg("index", sequence_index)
            .with_arg("start", sequence.start_frame)
            .with_arg("end", sequence.end_frame));
    }
    select_in_domain(
        &controller.keyframes,
        frame.sequence_frame,
        f64::from(sequence.start_frame),
        f64::from(sequence.end_frame),
        frame.playback,
    )
}

fn select_unbounded(keys: &[Keyframe], time: f64) -> Option<KeySelection<'_>> {
    if keys.is_empty() {
        return None;
    }
    if let Some(exact) = keys.iter().find(|key| f64::from(key.frame) == time) {
        return Some(KeySelection::Exact(exact));
    }
    let after_index = keys.partition_point(|key| f64::from(key.frame) < time);
    if after_index == 0 {
        Some(KeySelection::Single(&keys[0]))
    } else if after_index == keys.len() {
        Some(KeySelection::Single(keys.last().expect("non-empty keys")))
    } else {
        Some(between(
            &keys[after_index - 1],
            f64::from(keys[after_index - 1].frame),
            &keys[after_index],
            f64::from(keys[after_index].frame),
            time,
        ))
    }
}

fn select_in_domain(
    keys: &[Keyframe],
    time: f64,
    start: f64,
    end: f64,
    playback: PlaybackMode,
) -> Result<Option<KeySelection<'_>>, MdlError> {
    let active: Vec<_> = keys
        .iter()
        .filter(|key| (start..=end).contains(&f64::from(key.frame)))
        .collect();
    if active.is_empty() {
        return Ok(None);
    }
    if let Some(exact) = active
        .iter()
        .find(|key| f64::from(key.frame) == time)
        .copied()
    {
        return Ok(Some(KeySelection::Exact(exact)));
    }
    if active.len() == 1 {
        return Ok(Some(KeySelection::Single(active[0])));
    }
    let after_index = active.partition_point(|key| f64::from(key.frame) < time);
    if after_index > 0 && after_index < active.len() {
        return Ok(Some(between(
            active[after_index - 1],
            f64::from(active[after_index - 1].frame),
            active[after_index],
            f64::from(active[after_index].frame),
            time,
        )));
    }
    match playback {
        PlaybackMode::Clamp => Ok(Some(KeySelection::Single(if after_index == 0 {
            active[0]
        } else {
            active.last().expect("non-empty active keys")
        }))),
        PlaybackMode::Loop => {
            let period = end - start;
            if period == 0.0 {
                return Ok(Some(KeySelection::Single(active[0])));
            }
            let first = active[0];
            let last = *active.last().expect("non-empty active keys");
            if after_index == 0 {
                Ok(Some(between(
                    last,
                    f64::from(last.frame) - period,
                    first,
                    f64::from(first.frame),
                    time,
                )))
            } else {
                Ok(Some(between(
                    last,
                    f64::from(last.frame),
                    first,
                    f64::from(first.frame) + period,
                    time,
                )))
            }
        }
    }
}

fn between<'a>(
    before: &'a Keyframe,
    before_time: f64,
    after: &'a Keyframe,
    after_time: f64,
    time: f64,
) -> KeySelection<'a> {
    KeySelection::Between {
        before,
        after,
        t: ((time - before_time) / (after_time - before_time)) as f32,
    }
}

fn array_from_slice<const N: usize>(values: &[f32]) -> [f32; N] {
    std::array::from_fn(|index| values[index])
}

#[cfg(test)]
mod typed_sampling_tests {
    use super::*;
    use crate::animation::types::{PlaybackMode, ResolvedFrame};
    use crate::model::animation::Sequence;
    use crate::model::ids::TrackId;
    use crate::model::model::Model;
    use crate::model::objects::GlobalSequence;
    use crate::model::skeleton::{AnimationController, Keyframe};

    fn key(frame: i32, data: &[f32]) -> Keyframe {
        Keyframe {
            frame,
            data: data.to_vec(),
            in_tan: Vec::new(),
            out_tan: Vec::new(),
        }
    }

    fn tangent_key(frame: i32, data: &[f32], in_tan: &[f32], out_tan: &[f32]) -> Keyframe {
        Keyframe {
            frame,
            data: data.to_vec(),
            in_tan: in_tan.to_vec(),
            out_tan: out_tan.to_vec(),
        }
    }

    fn model(interpolation_type: u32, keys: Vec<Keyframe>) -> Model {
        Model {
            controllers: vec![AnimationController {
                interpolation_type,
                global_seq_id: -1,
                keyframes: keys,
            }],
            ..Model::default()
        }
    }

    fn frame(
        sequence: Option<usize>,
        sequence_frame: f64,
        playback: PlaybackMode,
    ) -> ResolvedFrame {
        ResolvedFrame {
            sequence,
            sequence_frame,
            global_frame: 0.0,
            playback,
            view: None,
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1.0e-5, "{actual} != {expected}");
    }

    fn assert_vec_close<const N: usize>(actual: [f32; N], expected: [f32; N]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_close(actual, expected);
        }
    }

    #[test]
    fn scalar_modes_follow_frozen_formulas() {
        let resolved = frame(None, 5.0, PlaybackMode::Clamp);
        let linear = model(1, vec![key(0, &[0.0]), key(10, &[10.0])]);
        assert_close(
            sample_scalar(&linear, TrackId(0), &resolved, 99.0).unwrap(),
            5.0,
        );

        let hermite = model(
            2,
            vec![
                tangent_key(0, &[0.0], &[1.0], &[4.0]),
                tangent_key(10, &[10.0], &[2.0], &[3.0]),
            ],
        );
        assert_close(
            sample_scalar(&hermite, TrackId(0), &resolved, 99.0).unwrap(),
            5.25,
        );

        let bezier = model(
            3,
            vec![
                tangent_key(0, &[0.0], &[1.0], &[4.0]),
                tangent_key(10, &[10.0], &[2.0], &[3.0]),
            ],
        );
        assert_close(
            sample_scalar(&bezier, TrackId(0), &resolved, 99.0).unwrap(),
            3.5,
        );

        let discrete = model(0, vec![key(0, &[3.0]), key(10, &[9.0])]);
        assert_eq!(
            sample_scalar(&discrete, TrackId(0), &resolved, 99.0).unwrap(),
            3.0
        );
    }

    #[test]
    fn vec3_modes_are_component_wise_and_fractional() {
        let resolved = frame(None, 2.5, PlaybackMode::Clamp);
        let linear = model(
            1,
            vec![key(0, &[0.0, 2.0, 4.0]), key(10, &[10.0, 12.0, 14.0])],
        );
        assert_vec_close(
            sample_vec3(&linear, TrackId(0), &resolved, [9.0; 3]).unwrap(),
            [2.5, 4.5, 6.5],
        );

        let bezier = model(
            3,
            vec![
                tangent_key(0, &[0.0; 3], &[0.0; 3], &[4.0; 3]),
                tangent_key(10, &[10.0; 3], &[2.0; 3], &[0.0; 3]),
            ],
        );
        assert_vec_close(
            sample_vec3(&bezier, TrackId(0), &resolved, [9.0; 3]).unwrap(),
            [2.125; 3],
        );
    }

    #[test]
    fn quaternion_linear_uses_shortest_normalized_slerp() {
        let resolved = frame(None, 5.0, PlaybackMode::Clamp);
        let rotation = model(
            1,
            vec![
                key(0, &[0.0, 0.0, 0.0, 1.0]),
                key(10, &[0.0, 0.0, 1.0, 0.0]),
            ],
        );
        assert_vec_close(
            sample_quaternion(&rotation, TrackId(0), &resolved, [9.0; 4]).unwrap(),
            [
                0.0,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ],
        );

        let antipodal = model(
            1,
            vec![
                key(0, &[0.0, 0.0, 0.0, 1.0]),
                key(10, &[0.0, 0.0, 0.0, -1.0]),
            ],
        );
        assert_vec_close(
            sample_quaternion(&antipodal, TrackId(0), &resolved, [9.0; 4]).unwrap(),
            [0.0, 0.0, 0.0, 1.0],
        );
    }

    #[test]
    fn quaternion_hermite_and_bezier_share_nested_slerpq() {
        let resolved = frame(None, 5.0, PlaybackMode::Clamp);
        for interpolation_type in [2, 3] {
            let rotation = model(
                interpolation_type,
                vec![
                    tangent_key(
                        0,
                        &[0.0, 0.0, 0.0, 1.0],
                        &[0.0, 0.0, 0.0, 1.0],
                        &[0.0, 0.0, 0.0, 1.0],
                    ),
                    tangent_key(
                        10,
                        &[0.0, 0.0, 1.0, 0.0],
                        &[0.0, 0.0, 0.0, 1.0],
                        &[0.0, 0.0, 0.0, 1.0],
                    ),
                ],
            );
            assert_vec_close(
                sample_quaternion(&rotation, TrackId(0), &resolved, [9.0; 4]).unwrap(),
                [0.0, 0.0, 0.38268343, 0.9238795],
            );
        }
    }

    #[test]
    fn sequence_loop_interpolates_across_tail_and_dont_interp_looks_back() {
        let mut linear = model(1, vec![key(120, &[2.0]), key(180, &[8.0])]);
        linear.sequences.push(Sequence {
            start_frame: 100,
            end_frame: 200,
            ..Sequence::default()
        });
        assert_close(
            sample_scalar(
                &linear,
                TrackId(0),
                &frame(Some(0), 190.0, PlaybackMode::Loop),
                0.0,
            )
            .unwrap(),
            6.5,
        );
        assert_close(
            sample_scalar(
                &linear,
                TrackId(0),
                &frame(Some(0), 110.0, PlaybackMode::Loop),
                0.0,
            )
            .unwrap(),
            3.5,
        );

        linear.controllers[0].interpolation_type = 0;
        assert_eq!(
            sample_scalar(
                &linear,
                TrackId(0),
                &frame(Some(0), 110.0, PlaybackMode::Loop),
                0.0
            )
            .unwrap(),
            8.0
        );

        linear.controllers[0].interpolation_type = 1;
        assert_close(
            sample_scalar(
                &linear,
                TrackId(0),
                &frame(Some(0), 90.0, PlaybackMode::Loop),
                0.0,
            )
            .unwrap(),
            6.5,
        );
        assert_close(
            sample_scalar(
                &linear,
                TrackId(0),
                &frame(Some(0), 210.0, PlaybackMode::Loop),
                0.0,
            )
            .unwrap(),
            3.5,
        );
    }

    #[test]
    fn sequence_clamp_does_not_cross_and_empty_activity_uses_default() {
        let mut tracked = model(1, vec![key(120, &[2.0]), key(180, &[8.0])]);
        tracked.sequences.push(Sequence {
            start_frame: 100,
            end_frame: 200,
            ..Sequence::default()
        });
        assert_eq!(
            sample_scalar(
                &tracked,
                TrackId(0),
                &frame(Some(0), 100.0, PlaybackMode::Clamp),
                0.0
            )
            .unwrap(),
            2.0
        );
        assert_eq!(
            sample_scalar(
                &tracked,
                TrackId(0),
                &frame(Some(0), 200.0, PlaybackMode::Clamp),
                0.0
            )
            .unwrap(),
            8.0
        );
        tracked.controllers[0].keyframes = vec![key(0, &[5.0]), key(50, &[6.0])];
        assert_eq!(
            sample_scalar(
                &tracked,
                TrackId(0),
                &frame(Some(0), 150.0, PlaybackMode::Clamp),
                77.0
            )
            .unwrap(),
            77.0
        );
    }

    #[test]
    fn sequence_none_is_unbounded_and_clamps_to_track_ends() {
        let tracked = model(1, vec![key(100, &[2.0]), key(200, &[8.0])]);
        assert_eq!(
            sample_scalar(
                &tracked,
                TrackId(0),
                &frame(None, 50.0, PlaybackMode::Loop),
                0.0
            )
            .unwrap(),
            2.0
        );
        assert_eq!(
            sample_scalar(
                &tracked,
                TrackId(0),
                &frame(None, 250.0, PlaybackMode::Loop),
                0.0
            )
            .unwrap(),
            8.0
        );
    }

    #[test]
    fn global_sequence_uses_independent_clock_and_zero_duration_first_key() {
        let mut tracked = model(1, vec![key(0, &[0.0]), key(100, &[10.0])]);
        tracked
            .global_sequences
            .push(GlobalSequence { duration: 100 });
        tracked.controllers[0].global_seq_id = 0;
        let mut resolved = frame(Some(99), 9999.0, PlaybackMode::Clamp);
        resolved.global_frame = 250.0;
        assert_close(
            sample_scalar(&tracked, TrackId(0), &resolved, 77.0).unwrap(),
            5.0,
        );
        for time in [-50.0, 350.0] {
            resolved.global_frame = time;
            assert_close(
                sample_scalar(&tracked, TrackId(0), &resolved, 77.0).unwrap(),
                5.0,
            );
        }
        resolved.global_frame = 100.0;
        assert_close(
            sample_scalar(&tracked, TrackId(0), &resolved, 77.0).unwrap(),
            0.0,
        );

        tracked.global_sequences[0].duration = 0;
        tracked.controllers[0].keyframes = vec![key(20, &[3.0]), key(40, &[9.0])];
        assert_eq!(
            sample_scalar(&tracked, TrackId(0), &resolved, 77.0).unwrap(),
            3.0
        );
        tracked.controllers[0].keyframes.clear();
        assert_eq!(
            sample_scalar(&tracked, TrackId(0), &resolved, 77.0).unwrap(),
            77.0
        );
    }

    #[test]
    fn typed_defaults_cover_none_empty_and_no_active_keys() {
        let resolved = frame(None, 0.0, PlaybackMode::Clamp);
        let empty = model(1, Vec::new());
        assert_eq!(
            sample_scalar(&empty, TrackId::NONE, &resolved, 2.0).unwrap(),
            2.0
        );
        assert_eq!(
            sample_vec3(&empty, TrackId(0), &resolved, [1.0, 2.0, 3.0]).unwrap(),
            [1.0, 2.0, 3.0]
        );
        assert_eq!(
            sample_quaternion(&empty, TrackId(0), &resolved, [0.0, 0.0, 0.0, 1.0]).unwrap(),
            [0.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            sample_discrete(&empty, TrackId(0), &resolved, 42).unwrap(),
            42
        );
        let mut invalid_but_empty = model(99, Vec::new());
        invalid_but_empty.controllers[0].global_seq_id = 99;
        assert_eq!(
            sample_scalar(&invalid_but_empty, TrackId(0), &resolved, 8.0).unwrap(),
            8.0
        );
    }

    #[test]
    fn exact_keys_return_original_values() {
        let tracked = model(
            1,
            vec![
                key(10, &[0.0, 0.0, 0.0, 2.0]),
                key(20, &[0.0, 0.0, 1.0, 0.0]),
            ],
        );
        assert_eq!(
            sample_quaternion(
                &tracked,
                TrackId(0),
                &frame(None, 10.0, PlaybackMode::Clamp),
                [9.0; 4]
            )
            .unwrap(),
            [0.0, 0.0, 0.0, 2.0]
        );

        let mut sequence_end = model(1, vec![key(100, &[1.0]), key(200, &[9.0])]);
        sequence_end.sequences.push(Sequence {
            start_frame: 100,
            end_frame: 200,
            ..Sequence::default()
        });
        assert_eq!(
            sample_scalar(
                &sequence_end,
                TrackId(0),
                &frame(Some(0), 200.0, PlaybackMode::Loop),
                0.0,
            )
            .unwrap(),
            9.0
        );
    }

    #[test]
    fn controller_index_interpolation_and_order_errors_are_stable() {
        let resolved = frame(None, 0.0, PlaybackMode::Clamp);
        assert_eq!(
            sample_scalar(&Model::default(), TrackId(0), &resolved, 0.0)
                .unwrap_err()
                .key,
            "animation-invalid-controller-index"
        );
        let invalid_mode = model(4, vec![key(0, &[1.0])]);
        assert_eq!(
            sample_scalar(&invalid_mode, TrackId(0), &resolved, 0.0)
                .unwrap_err()
                .key,
            "animation-invalid-interpolation-type"
        );
        let bad_order = model(1, vec![key(10, &[1.0]), key(10, &[2.0])]);
        assert_eq!(
            sample_scalar(&bad_order, TrackId(0), &resolved, 0.0)
                .unwrap_err()
                .key,
            "animation-invalid-keyframe-order"
        );

        let mut bad_sequence = model(1, vec![key(0, &[1.0])]);
        assert_eq!(
            sample_scalar(
                &bad_sequence,
                TrackId(0),
                &frame(Some(0), 0.0, PlaybackMode::Clamp),
                0.0,
            )
            .unwrap_err()
            .key,
            "animation-invalid-sequence-index"
        );
        bad_sequence.sequences.push(Sequence {
            start_frame: 10,
            end_frame: 0,
            ..Sequence::default()
        });
        assert_eq!(
            sample_scalar(
                &bad_sequence,
                TrackId(0),
                &frame(Some(0), 0.0, PlaybackMode::Clamp),
                0.0,
            )
            .unwrap_err()
            .key,
            "animation-invalid-sequence-range"
        );
        assert_eq!(
            sample_scalar(
                &bad_sequence,
                TrackId(0),
                &frame(None, f64::NAN, PlaybackMode::Clamp),
                0.0,
            )
            .unwrap_err()
            .key,
            "animation-invalid-frame-time"
        );
        assert_eq!(
            sample_scalar(
                &bad_sequence,
                TrackId(0),
                &frame(None, f64::INFINITY, PlaybackMode::Clamp),
                0.0,
            )
            .unwrap_err()
            .key,
            "animation-invalid-frame-time"
        );
    }

    #[test]
    fn width_tangent_and_non_finite_errors_are_stable() {
        let resolved = frame(None, 5.0, PlaybackMode::Clamp);
        let bad_width = model(1, vec![key(0, &[1.0, 2.0]), key(10, &[3.0, 4.0])]);
        assert_eq!(
            sample_scalar(&bad_width, TrackId(0), &resolved, 0.0)
                .unwrap_err()
                .key,
            "animation-invalid-track-width"
        );
        let bad_tangent = model(
            2,
            vec![
                tangent_key(0, &[1.0], &[], &[1.0]),
                tangent_key(10, &[2.0], &[1.0], &[1.0]),
            ],
        );
        assert_eq!(
            sample_scalar(&bad_tangent, TrackId(0), &resolved, 0.0)
                .unwrap_err()
                .key,
            "animation-invalid-tangent-width"
        );
        let non_finite = model(1, vec![tangent_key(0, &[1.0], &[], &[f32::NAN])]);
        assert_eq!(
            sample_scalar(&non_finite, TrackId(0), &resolved, 0.0)
                .unwrap_err()
                .key,
            "animation-non-finite-track-value"
        );
    }

    #[test]
    fn invalid_quaternions_and_global_ids_are_errors() {
        let resolved = frame(None, 0.0, PlaybackMode::Clamp);
        let zero = model(1, vec![key(0, &[0.0; 4])]);
        assert_eq!(
            sample_quaternion(&zero, TrackId(0), &resolved, [0.0, 0.0, 0.0, 1.0])
                .unwrap_err()
                .key,
            "animation-invalid-quaternion"
        );
        let non_finite = model(1, vec![key(0, &[0.0, 0.0, f32::NAN, 1.0])]);
        assert_eq!(
            sample_quaternion(&non_finite, TrackId(0), &resolved, [0.0, 0.0, 0.0, 1.0])
                .unwrap_err()
                .key,
            "animation-invalid-quaternion"
        );
        let mut bad_global = model(1, vec![key(0, &[1.0])]);
        bad_global.controllers[0].global_seq_id = 0;
        assert_eq!(
            sample_scalar(&bad_global, TrackId(0), &resolved, 0.0)
                .unwrap_err()
                .key,
            "animation-invalid-global-sequence-index"
        );
    }

    #[test]
    fn discrete_uses_bankers_rounding_and_checks_range() {
        let resolved = frame(None, 0.0, PlaybackMode::Clamp);
        for (value, expected) in [(2.5, 2), (3.5, 4), (-2.5, -2)] {
            let tracked = model(1, vec![key(0, &[value])]);
            assert_eq!(
                sample_discrete(&tracked, TrackId(0), &resolved, 0).unwrap(),
                expected
            );
        }
        let out_of_range = model(1, vec![key(0, &[3.0e9])]);
        assert_eq!(
            sample_discrete(&out_of_range, TrackId(0), &resolved, 0)
                .unwrap_err()
                .key,
            "animation-discrete-value-out-of-range"
        );
    }
}
