//! Reading and reaching into an element's effect chain and its transition.

use super::*;

pub fn effects_mut(element: &mut TimelineElement) -> Option<&mut Option<Vec<Effect>>> {
    match element {
        TimelineElement::Video(inner) => Some(&mut inner.effects),
        TimelineElement::Image(inner) => Some(&mut inner.effects),
        TimelineElement::Text(inner) => Some(&mut inner.effects),
        TimelineElement::Sticker(inner) => Some(&mut inner.effects),
        TimelineElement::Graphic(inner) => Some(&mut inner.effects),
        _ => None,
    }
}

pub(crate) fn effect_mut<'a>(
    element: &'a mut TimelineElement,
    effect_id: &str,
) -> Option<&'a mut Effect> {
    effects_mut(element)?
        .as_mut()?
        .iter_mut()
        .find(|effect| effect.id == effect_id)
}

pub fn effects_of(element: &TimelineElement) -> &[Effect] {
    let effects = match element {
        TimelineElement::Video(inner) => inner.effects.as_ref(),
        TimelineElement::Image(inner) => inner.effects.as_ref(),
        TimelineElement::Text(inner) => inner.effects.as_ref(),
        TimelineElement::Sticker(inner) => inner.effects.as_ref(),
        TimelineElement::Graphic(inner) => inner.effects.as_ref(),
        _ => None,
    };
    effects.map(|effects| effects.as_slice()).unwrap_or(&[])
}

pub fn transition_of(element: &TimelineElement) -> Option<&ElementTransition> {
    match element {
        TimelineElement::Video(inner) => inner.transition.as_ref(),
        TimelineElement::Image(inner) => inner.transition.as_ref(),
        _ => None,
    }
}
