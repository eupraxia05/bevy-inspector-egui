use super::InspectorPrimitive;
use bevy_asset::{meta::AssetMetaMinimal, Asset, Assets, Handle};
use bevy_reflect::DynamicTypePath;
use super::InspectorUi;
use crate::bevy_inspector::errors::{show_error, no_world_in_context};
use pretty_type_name::pretty_type_name;
use bevy_asset::AssetServer;
use bevy_ecs::prelude::Res;
use bevy_tasks::{AsyncComputeTaskPool, TaskPool, futures_lite::StreamExt, FakeTask};
use bevy_asset::io::AssetSourceId;
use std::path::{Path, PathBuf};
use bevy_log::{info, error};
use bevy_ecs::prelude::{Commands, Component};
use std::any::TypeId;
use bevy_ecs::prelude::{World, Resource};

pub(crate) fn handle_ui<T: Asset>(handle: &mut Handle<T>, ui: &mut egui::Ui, 
  id: egui::Id, env: &mut InspectorUi<'_, '_>) -> bool 
{
  let Some(world) = &mut env.context.world else {
    let immutable_handle: &Handle<T> = handle;
    no_world_in_context(ui, immutable_handle.reflect_short_type_path());
    return false;
  };

  let (asset_server, asset_data) =
  match world.get_two_resources_mut::<bevy_asset::AssetServer, AssetData>() {
      (Ok(a), Ok(b)) => (a, b),
      (a, b) => {
          if let Err(e) = a {
              show_error(e, ui, &pretty_type_name::<bevy_asset::AssetServer>());
          }
          if let Err(e) = b {
              show_error(e, ui, &pretty_type_name::<AssetData>());
          }
          return false;
      }
  };

  // get all loaded image paths
  let mut asset_paths = Vec::new();
  for asset in asset_data.assets.iter() {
    if asset.asset_type == TypeId::of::<T>() {
      asset_paths.push(asset.path.to_str().unwrap().to_string());
    }
  }

  // first, get the typed search text from a stored egui data value
  let mut selected_path = None;
  let mut image_picker_search_text = String::from("");
  ui.data_mut(|data| {
      image_picker_search_text = data
          .get_temp_mut_or_default::<String>(id.with("image_picker_search_text"))
          .clone();
  });

  // build and show the dropdown
  let dropdown = egui_dropdown::DropDownBox::from_iter(
      asset_paths.iter(),
      id.with("image_picker"),
      &mut image_picker_search_text,
      |ui, path| {
          let response = ui.selectable_label(false, path);
          if response.clicked() {
              selected_path = Some(path.to_string());
          }
          response
      },
  );
  ui.add(dropdown);

  // update the typed search text
  ui.data_mut(|data| {
      *data.get_temp_mut_or_default::<String>(id.with("image_picker_search_text")) =
          image_picker_search_text;
  });

  // if the user selected an option, update the image handle
  if let Some(selected_path) = selected_path {
      info!("setting handle");
      *handle = asset_server.load(selected_path);
  }

  false
}

#[derive(Clone)]
struct AssetEntry {
  path: PathBuf,
  asset_type: TypeId
}

#[derive(Resource, Clone)]
struct AssetData {
  assets: Vec<AssetEntry>
}

pub(crate) fn update_assets(world: &mut World) {
  let thread_pool = AsyncComputeTaskPool::get();
  let server = world.resource::<AssetServer>().clone();
  thread_pool.scope(|spawner| {
    spawner.spawn(async {
      let mut assets = Vec::new();
      let source = server.get_source(AssetSourceId::Default).unwrap();
      let reader = source.reader();
      let dir_read = reader.read_directory(&PathBuf::from("")).await;
      match dir_read {
        Ok(mut stream) => {
          while let Some(path) = stream.next().await {
            if let Ok(loader) = server.get_path_asset_loader(path.clone()).await {
              assets.push(AssetEntry{path, asset_type: loader.asset_type_id()});
            }
          }
        },
        Err(e) => {
          info!("Failed to open asset root dir: {}", e);
        }
      }
      info!("{}", assets.len());
      world.insert_resource(AssetData{assets});
    })
  });
}
