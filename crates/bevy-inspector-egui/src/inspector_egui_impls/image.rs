use std::{
    any::Any,
    collections::{hash_map::Entry, HashMap, HashSet},
    sync::Mutex,
};

use bevy_asset::{Assets, Handle};
use bevy_egui::EguiUserTextures;
use bevy_reflect::DynamicTypePath;
use bevy_render::texture::Image;
use egui::load::SizedTexture;
use once_cell::sync::Lazy;
use pretty_type_name::pretty_type_name;

use crate::{
    bevy_inspector::errors::{no_world_in_context, show_error},
    reflect_inspector::InspectorUi,
    restricted_world_view::RestrictedWorldView,
};

use super::InspectorPrimitive;
use super::handle::handle_ui;

mod image_texture_conversion;

impl InspectorPrimitive for Handle<Image> {
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _: &dyn Any,
        id: egui::Id,
        mut env: InspectorUi<'_, '_>,
    ) -> bool {
        let Some(world) = &mut env.context.world else {
            let immutable_self: &Handle<Image> = self;
            no_world_in_context(ui, immutable_self.reflect_short_type_path());
            return false;
        };

        update_and_show_image(self, world, ui);

        handle_ui(self, ui, id, &mut env)
    }

    fn ui_readonly(&self, ui: &mut egui::Ui, _: &dyn Any, _: egui::Id, env: InspectorUi<'_, '_>) {
        let Some(world) = &mut env.context.world else {
            no_world_in_context(ui, self.reflect_short_type_path());
            return;
        };

        update_and_show_image(self, world, ui);
    }
}

static SCALED_DOWN_TEXTURES: Lazy<Mutex<ScaledDownTextures>> = Lazy::new(Default::default);

fn update_and_show_image(
    image: &Handle<Image>,
    world: &mut RestrictedWorldView,
    ui: &mut egui::Ui,
) {
    let (mut egui_user_textures, mut images) =
        match world.get_two_resources_mut::<bevy_egui::EguiUserTextures, Assets<Image>>() {
            (Ok(a), Ok(b)) => (a, b),
            (a, b) => {
                if let Err(e) = a {
                    show_error(e, ui, &pretty_type_name::<bevy_egui::EguiContext>());
                }
                if let Err(e) = b {
                    show_error(e, ui, &pretty_type_name::<Assets<Image>>());
                }
                return;
            }
        };

    let mut scaled_down_textures = SCALED_DOWN_TEXTURES.lock().unwrap();

    // todo: read asset events to re-rescale images of they changed
    let rescaled = rescaled_image(
        image,
        &mut scaled_down_textures,
        &mut images,
        &mut egui_user_textures,
    );
    let (rescaled_handle, texture_id) = match rescaled {
        Some(it) => it,
        None => {
            ui.label("<texture>");
            return;
        }
    };

    let rescaled_image = images.get(&rescaled_handle).unwrap();
    show_image(rescaled_image, texture_id, ui);
}

fn show_image(
    image: &Image,
    texture_id: egui::TextureId,
    ui: &mut egui::Ui,
) -> Option<egui::Response> {
    let size = image.texture_descriptor.size;
    let size = egui::Vec2::new(size.width as f32, size.height as f32);

    let source = SizedTexture {
        id: texture_id,
        size,
    };

    if size.max_elem() >= 128.0 {
        let response = egui::CollapsingHeader::new("Texture").show(ui, |ui| ui.image(source));
        response.body_response
    } else {
        let response = ui.image(source);
        Some(response)
    }
}

#[derive(Default)]
struct ScaledDownTextures {
    textures: HashMap<Handle<Image>, Handle<Image>>,
    rescaled_textures: HashSet<Handle<Image>>,
}

const RESCALE_TO_FIT: (u32, u32) = (100, 100);

fn rescaled_image<'a>(
    handle: &Handle<Image>,
    scaled_down_textures: &'a mut ScaledDownTextures,
    textures: &mut Assets<Image>,
    egui_usere_textures: &mut EguiUserTextures,
) -> Option<(Handle<Image>, egui::TextureId)> {
    let (texture, texture_id) = match scaled_down_textures.textures.entry(handle.clone()) {
        Entry::Occupied(handle) => {
            let handle: Handle<Image> = handle.get().clone();
            (handle.clone(), egui_usere_textures.add_image(handle))
        }
        Entry::Vacant(entry) => {
            if scaled_down_textures.rescaled_textures.contains(handle) {
                return None;
            }

            let original = textures.get(handle)?;

            let (image, is_srgb) = image_texture_conversion::try_into_dynamic(original)?;
            let resized = image.resize(
                RESCALE_TO_FIT.0,
                RESCALE_TO_FIT.1,
                image::imageops::FilterType::Triangle,
            );
            let resized = image_texture_conversion::from_dynamic(resized, is_srgb);

            let resized_handle = textures.add(resized);
            let weak = resized_handle.clone_weak();
            let texture_id = egui_usere_textures.add_image(resized_handle.clone());
            entry.insert(resized_handle);
            scaled_down_textures.rescaled_textures.insert(weak.clone());

            (weak, texture_id)
        }
    };

    Some((texture, texture_id))
}
