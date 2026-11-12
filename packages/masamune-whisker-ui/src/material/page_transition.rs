//! Page transitions for `whisker-router` routes (Flutter's `PageTransitionsBuilder`).

use whisker::AnimConfig;
use whisker::css::{Transform, TransformFn};
use whisker::prelude::*;
use whisker_router::{Pose, PoseContext, Role, RouteTransition, Transition};

/// Duration (ms) of [`FadeUpwardsTransition`].
pub const FADE_UPWARDS_MS: u32 = 300;

/// How far (percent of the screen height) the top screen sits below its resting position when it
/// is fully off screen.
pub const FADE_UPWARDS_OFFSET_PCT: f32 = 10.0;

/// Full-screen page transition (Flutter's `FadeUpwardsPageTransitionsBuilder`): the new screen
/// rises a little from below while fading in, and sinks back while fading out on back. The screen
/// beneath it stays still.
///
/// ```ignore
/// Route(path: "items/:id", component: ItemPage, transition: fade_upwards_transition())
/// ```
pub struct FadeUpwardsTransition;

impl Transition for FadeUpwardsTransition {
    fn config(&self) -> AnimConfig {
        AnimConfig::ease_out(FADE_UPWARDS_MS)
    }

    fn name(&self) -> &'static str {
        "fade-upwards"
    }

    fn pose(&self, ctx: PoseContext) -> Pose {
        let p = ctx.progress.clamp(0.0, 1.0);
        match ctx.role {
            Role::Top => {
                let y = (1.0 - p) * FADE_UPWARDS_OFFSET_PCT;
                Pose::new(
                    Transform::new().push(TransformFn::TranslateY(percent(y).into())),
                    p,
                )
            }
            Role::Under => Pose::new(Transform::new(), 1.0),
        }
    }
}

/// [`FadeUpwardsTransition`] as a [`RouteTransition`] for a route's `transition:`.
pub fn fade_upwards_transition() -> RouteTransition {
    RouteTransition::custom(FadeUpwardsTransition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker_router::Direction;

    fn pose(role: Role, progress: f32) -> Pose {
        fade_upwards_transition().pose(PoseContext::new(role, progress, Direction::Push))
    }

    #[test]
    fn top_rises_and_fades_in() {
        let hidden = pose(Role::Top, 0.0);
        assert_eq!(hidden.opacity, 0.0);
        assert_eq!(
            hidden.transform,
            Transform::new().push(TransformFn::TranslateY(
                percent(FADE_UPWARDS_OFFSET_PCT).into()
            ))
        );

        let shown = pose(Role::Top, 1.0);
        assert_eq!(shown.opacity, 1.0);
        assert_eq!(
            shown.transform,
            Transform::new().push(TransformFn::TranslateY(percent(0.0).into()))
        );
    }

    #[test]
    fn under_stays_still() {
        for progress in [0.0, 0.5, 1.0] {
            let under = pose(Role::Under, progress);
            assert_eq!(under.opacity, 1.0);
            assert_eq!(under.transform, Transform::new());
        }
    }

    #[test]
    fn named_fade_upwards() {
        assert_eq!(fade_upwards_transition().name(), "fade-upwards");
    }
}
