//! Building new elements that are not driven by a media asset.

use super::*;

pub(crate) fn new_base(name: String) -> BaseElementFields {
    BaseElementFields {
        id: new_id(),
        name,
        duration: seconds(DEFAULT_NEW_ELEMENT_SECONDS),
        start_time: MediaTime::ZERO,
        trim_start: MediaTime::ZERO,
        trim_end: MediaTime::ZERO,
        source_duration: None,
        animations: None,
    }
}

pub fn sticker_element(sticker_id: String, name: String) -> TimelineElement {
    let (intrinsic_width, intrinsic_height) = stickers::sticker_intrinsic_size(&sticker_id)
        .unwrap_or((
            stickers::DEFAULT_INTRINSIC_SIZE,
            stickers::DEFAULT_INTRINSIC_SIZE,
        ));

    TimelineElement::Sticker(cutix_project::model::StickerElement {
        base: new_base(name),
        sticker_id,
        intrinsic_width: Some(intrinsic_width),
        intrinsic_height: Some(intrinsic_height),
        hidden: None,
        transform: Transform::default(),
        crop: None,
        opacity: 1.0,
        blend_mode: None,
        effects: None,
        cutout: None,
        motion: None,
        extra: JsonMap::new(),
    })
}

pub fn graphic_element(
    definition_id: String,
    name: String,
    overrides: cutix_project::model::ParamValues,
) -> TimelineElement {
    let params = stickers::resolve_params(&definition_id, &overrides);
    TimelineElement::Graphic(cutix_project::model::GraphicElement {
        base: new_base(name),
        definition_id,
        params,
        hidden: None,
        transform: Transform::default(),
        crop: None,
        opacity: 1.0,
        blend_mode: None,
        effects: None,
        masks: None,
        extra: JsonMap::new(),
    })
}
