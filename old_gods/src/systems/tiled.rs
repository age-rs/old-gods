//! The TiledSystem loads Tiled map files and decomposes them into objects in the
//! ECS.
//!
//! Once the objects are injected in the ECS it's up to other systems to modify
//! and replace them.
use super::super::{
    fetch,
    prelude::{
        Animation, Barrier, CanBeEmpty, Component, Either, Entities, Entity, Fence, Frame,
        GlobalTileIndex, HashMapStorage, Join, Layer, LayerData, LoadStatus, LoadableResources,
        Name, Object, ObjectGroup, ObjectLayerData, ObjectRenderingToggles, OriginOffset, Position,
        Rendering, RenderingToggles, ResourceId, Resources, Shape, SharedResource, StepFence,
        System, SystemData, TextureFrame, TileLayerData, Tiledmap, World, WriteStorage, ZLevel,
        Zone, JSON, V2,
    },
    resources,
};
use log::{trace, warn};
use serde_json::Value;
use std::{collections::HashMap, iter::FromIterator};
use wasm_bindgen_futures::spawn_local;


pub struct TiledmapResources {
    base_url: String,
    loads: LoadableResources<Tiledmap>,
}


async fn load_map_wasm(base_url: &str, path: &str, shared: SharedResource<Tiledmap>) {
    match Tiledmap::from_url(base_url, path, fetch::from_url).await {
        Ok(map) => {
            shared.set_status_and_resource((LoadStatus::Complete, Some(map)));
        }
        Err(err) => {
            shared.set_status(LoadStatus::Error(err));
        }
    }
}


impl TiledmapResources {
    fn new(base_url: &str) -> Self {
        TiledmapResources {
            base_url: base_url.to_string(),
            loads: LoadableResources::new(),
        }
    }
}


impl Resources<Tiledmap> for TiledmapResources {
    fn status_of(&self, key: &str) -> LoadStatus {
        self.loads.status_of(key)
    }

    fn load(&mut self, path: &str) {
        trace!("loading map '{}'", path);
        let path = path.to_string();
        let shared = SharedResource::default();
        self.loads.resources.insert(path.clone(), shared.clone());
        let base_url = self.base_url.clone();

        spawn_local(async move { load_map_wasm(&base_url, &path, shared).await });
    }

    fn take(&mut self, path: &str) -> Option<SharedResource<Tiledmap>> {
        self.loads.take(path)
    }

    fn put(&mut self, path: &str, map: SharedResource<Tiledmap>) {
        self.loads.put(path, map)
    }
}


impl Default for TiledmapResources {
    fn default() -> Self {
        TiledmapResources::new("")
    }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadMap {
    pub file: String,
}


impl Component for LoadMap {
    type Storage = HashMapStorage<Self>;
}


/// Return a rendering for the tile with the given GlobalId.
pub fn get_rendering(
    tm: &Tiledmap,
    gid: &GlobalTileIndex,
    // TODO: Instead of providing a size to get_[rendering|anime] we can
    // alter the size of the texture frame after determining the scale of an
    // object.
    size: Option<(u32, u32)>,
) -> Option<Rendering> {
    let (firstgid, tileset) = tm.get_tileset_by_gid(&gid.id)?;
    let aabb = tileset.aabb(firstgid, &gid.id)?;
    Some(Rendering::from_frame(TextureFrame {
        sprite_sheet: tileset.image.clone(),
        source_aabb: aabb.clone(),
        size: size.unwrap_or((aabb.w, aabb.h)),
        is_flipped_horizontally: gid.is_flipped_horizontally,
        is_flipped_vertically: gid.is_flipped_vertically,
        is_flipped_diagonally: gid.is_flipped_diagonally,
    }))
}


pub fn get_animation(
    tm: &Tiledmap,
    gid: &GlobalTileIndex,
    // TODO: Instead of providing a size to get_[rendering|anime] we can
    // alter the size of the texture frame after determining the scale of an
    // object.
    size: Option<(u32, u32)>,
) -> Option<Animation> {
    let (firstgid, tileset) = tm.get_tileset_by_gid(&gid.id)?;
    let tile = tileset.tile(firstgid, &gid.id)?;
    // Get out the animation frames
    let frames = tile.clone().animation?;
    Some(Animation {
        is_playing: true,
        frames: Vec::from_iter(frames.iter().filter_map(|frame| {
            tileset.aabb_local(&frame.tileid).map(|frame_aabb| {
                let size = size.unwrap_or((frame_aabb.w, frame_aabb.h));
                Frame {
                    rendering: Rendering::from_frame(TextureFrame {
                        sprite_sheet: tileset.image.clone(),
                        source_aabb: frame_aabb,
                        size,
                        is_flipped_horizontally: gid.is_flipped_horizontally,
                        is_flipped_vertically: gid.is_flipped_vertically,
                        is_flipped_diagonally: gid.is_flipped_diagonally,
                    }),
                    duration: frame.duration as f32 / 1000.0,
                }
            })
        })),
        current_frame_index: 0,
        current_frame_progress: 0.0,
        should_repeat: true,
    })
}


/// Add an origin component to the entity.
fn add_origin(ent: Entity, x: f32, y: f32, offsets: &mut WriteStorage<OriginOffset>) {
    let _ = offsets.insert(ent, OriginOffset(V2::new(x, y)));
}


pub fn add_barrier(
    ent: Entity,
    obj: &Object,
    barriers: &mut WriteStorage<Barrier>,
    shapes: &mut WriteStorage<Shape>,
) {
    let _ = barriers.insert(ent, Barrier);
    let _ = shapes.insert(
        ent,
        Shape::Box {
            lower: V2::new(obj.x, obj.y),
            upper: V2::new(obj.x + obj.width, obj.y + obj.height),
        },
    );
}


pub struct TiledmapSystem {
    resources: TiledmapResources,
}


impl TiledmapSystem {
    pub fn new(base_url: &str) -> Self {
        TiledmapSystem {
            resources: TiledmapResources::new(base_url),
        }
    }
}


#[derive(SystemData)]
pub struct InsertMapData<'s> {
    entities: Entities<'s>,
    animations: WriteStorage<'s, Animation>,
    barriers: WriteStorage<'s, Barrier>,
    fences: WriteStorage<'s, Fence>,
    jsons: WriteStorage<'s, JSON>,
    names: WriteStorage<'s, Name>,
    objects: WriteStorage<'s, Object>,
    object_toggles: WriteStorage<'s, ObjectRenderingToggles>,
    offsets: WriteStorage<'s, OriginOffset>,
    positions: WriteStorage<'s, Position>,
    renderings: WriteStorage<'s, Rendering>,
    shapes: WriteStorage<'s, Shape>,
    step_fences: WriteStorage<'s, StepFence>,
    zlevels: WriteStorage<'s, ZLevel>,
    zones: WriteStorage<'s, Zone>,
}


type TiledmapSystemData<'s> = (Entities<'s>, WriteStorage<'s, LoadMap>, InsertMapData<'s>);


pub fn insert_map(map: &Tiledmap, data: &mut InsertMapData) {
    //trace!(
    //  "inserting tiled v{} map, {}x{}",
    //  map.tiledversion,
    //  map.width,
    //  map.height
    //);

    //// Pre process the layers into layers of tiles and objects.
    fn flatten_layers(layers_in: &[Layer]) -> Vec<Either<&TileLayerData, &ObjectLayerData>> {
        let mut layers_out = vec![];

        for layer in layers_in.iter() {
            match layer.type_is.as_ref() {
                "tilelayer" => {}
                "objectgroup" => {}
                "imagelayer" => {}
                t => {
                    warn!("found unsupported layer type '{}'", t);
                }
            }
            match &layer.layer_data {
                LayerData::Tiles(tiles) => {
                    layers_out.push(Either::Left(tiles));
                }
                LayerData::Objects(objects) => {
                    layers_out.push(Either::Right(objects));
                }
                LayerData::Layers(layers) => {
                    let tobjs = flatten_layers(&layers.layers);
                    layers_out.extend(tobjs);
                }
            }
        }
        layers_out
    };

    // Here's an empty vec just in case we need a ref to an empty vec (we do).
    let empty_vec = vec![];
    // Insert the flattened layers of tiles and objects
    let mut z = 0;
    for layer in flatten_layers(&map.layers) {
        match layer {
            Either::Left(TileLayerData {
                width: _tiles_x,
                height: _tiles_y,
                data: tiles,
            }) => {
                for (global_ndx, local_ndx) in tiles.iter().zip(0..) {
                    let tile_ent = data.entities.create();
                    let _ = data.zlevels.insert(tile_ent, ZLevel(z as f32));

                    let yndx = local_ndx / map.width;
                    let xndx = local_ndx % map.width;
                    let origin = V2::new(
                        (map.tilewidth * xndx) as f32,
                        (map.tileheight * yndx) as f32,
                    );
                    let _ = data.positions.insert(tile_ent, Position(origin));
                    if let Some(rendering) = get_rendering(map, &global_ndx, None) {
                        let _ = data.renderings.insert(tile_ent, rendering);
                    }
                    if let Some(anime) = get_animation(map, &global_ndx, None) {
                        let _ = data.animations.insert(tile_ent, anime);
                    }

                    if let Some(tile) = map.get_tile(&global_ndx.id) {
                        let mut properties = tile
                            .properties
                            .iter()
                            .map(|prop| (prop.name.clone(), prop.clone()))
                            .collect::<HashMap<_, _>>();

                        if let Some(debug_toggles) =
                            RenderingToggles::remove_from_properties(&mut properties)
                        {
                            let _ = data.object_toggles.insert(tile_ent, debug_toggles);
                        }

                        for obj in tile
                            .object_group
                            .as_ref()
                            .map(|group: &ObjectGroup| &group.objects)
                            .unwrap_or(&empty_vec)
                            .iter()
                        {
                            match obj.type_is.as_str() {
                                "origin_offset" => {
                                    add_origin(tile_ent, obj.x, obj.y, &mut data.offsets)
                                }
                                "barrier" => {
                                    add_barrier(tile_ent, obj, &mut data.barriers, &mut data.shapes)
                                }
                                "shape" => {
                                    let lower = V2::new(obj.x, obj.y);
                                    let upper = lower + V2::new(obj.width, obj.height);
                                    let shape = Shape::Box { lower, upper };
                                    let _ = data.shapes.insert(tile_ent, shape);
                                }
                                t => {
                                    panic!("unsupported object type within a tile: '{}'", t);
                                }
                            }
                        }
                    }
                }
            }

            Either::Right(ObjectLayerData { objects, .. }) => {
                for obj in objects.iter() {
                    let obj_ent = data.entities.create();
                    let _ = data.zlevels.insert(obj_ent, ZLevel(z as f32));
                    if let Some(name) = obj.name.non_empty() {
                        let _ = data.names.insert(obj_ent, Name(name.clone()));
                    }
                    if let Some(global_ndx) = &obj.gid {
                        let obj_pos = V2::new(obj.x, obj.y - obj.height);
                        let _ = data.positions.insert(obj_ent, Position(obj_pos));

                        // It's always a rectangle!
                        let lower = V2::origin();
                        let upper = V2::new(obj.width, obj.height);
                        let shape = Shape::Box { lower, upper };
                        let _ = data.shapes.insert(obj_ent, shape);

                        if let Some(rendering) = get_rendering(map, &global_ndx, None) {
                            let _ = data.renderings.insert(obj_ent, rendering);
                        }
                        if let Some(anime) = get_animation(map, &global_ndx, None) {
                            let _ = data.animations.insert(obj_ent, anime);
                        }

                        for sub_obj in map
                            .get_tile(&global_ndx.id)
                            .map(|tile| tile.object_group.as_ref())
                            .flatten()
                            .map(|group: &ObjectGroup| &group.objects)
                            .unwrap_or(&empty_vec)
                            .iter()
                        {
                            match sub_obj.type_is.as_str() {
                                "origin_offset" => {
                                    add_origin(obj_ent, sub_obj.x, sub_obj.y, &mut data.offsets)
                                }
                                "barrier" => add_barrier(
                                    obj_ent,
                                    sub_obj,
                                    &mut data.barriers,
                                    &mut data.shapes,
                                ),
                                "shape" => {
                                    let lower = V2::new(sub_obj.x, sub_obj.y);
                                    let upper = lower + V2::new(sub_obj.width, sub_obj.height);
                                    let shape = Shape::Box { lower, upper };
                                    let _ = data.shapes.insert(obj_ent, shape);
                                }
                                t => {
                                    panic!("unsupported sub-object type: '{}'", t);
                                }
                            }
                        }
                    } else {
                        // The object is not a tile
                        // Create its Position
                        let _ = data
                            .positions
                            .insert(obj_ent, Position(V2::new(obj.x, obj.y)));
                        // Create its Shape
                        if let Some(_polyline) = &obj.polyline {
                            // Probably a fence, handled below
                        } else if let Some(polygon) = &obj.polygon {
                            // Polygon
                            let vertices = polygon.iter().map(|p| V2::new(p.x, p.y)).collect();
                            let shape = Shape::Polygon { vertices };
                            let _ = data.shapes.insert(obj_ent, shape);
                        } else {
                            // Rectangle
                            let _ = data.shapes.insert(
                                obj_ent,
                                Shape::Box {
                                    lower: V2::origin(),
                                    upper: V2::new(obj.width, obj.height),
                                },
                            );
                        }
                    }

                    let mut properties = obj
                        .properties
                        .iter()
                        .map(|p| (p.name.clone(), p.clone()))
                        .collect();

                    if let Some(debug_toggles) =
                        RenderingToggles::remove_from_properties(&mut properties)
                    {
                        let _ = data.object_toggles.insert(obj_ent, debug_toggles);
                    }

                    let mut properties: HashMap<String, Value> =
                        properties.into_iter().map(|(k, p)| (k, p.value)).collect();

                    match obj.get_deep_type(map).as_str() {
                        //"sprite" => Sprite::read(self, map, object),
                        "zone" => {
                            let _ = data.zones.insert(obj_ent, Zone { inside: vec![] });
                        }

                        "fence" => {
                            if let Some(polyline) = &obj.polyline {
                                let _ = data.fences.insert(
                                    obj_ent,
                                    Fence::new(
                                        polyline.iter().map(|p| V2::new(p.x, p.y)).collect(),
                                    ),
                                );
                            } else {
                                panic!("a fence must be a polyline");
                            }
                        }
                        "step_fence" => {
                            if let Some(polyline) = &obj.polyline {
                                let _ = data.step_fences.insert(
                                    obj_ent,
                                    StepFence {
                                        step: properties
                                            .remove("step")
                                            .map(|v| v.as_f64().map(|f| f as f32))
                                            .flatten()
                                            .expect(
                                                "StepFence must have a proprety 'step' with a \
                                                 float value",
                                            ),
                                        fence: Fence::new(
                                            polyline.iter().map(|p| V2::new(p.x, p.y)).collect(),
                                        ),
                                    },
                                );
                            } else {
                                panic!("a fence must be a polyline");
                            }
                        }

                        //"point" | "sound" | "music" => {
                        //  let mut attributes = Attributes::read(map, object)?;
                        //  attributes.position_mut().map(|p| {
                        //    p.0 += self.origin;
                        //  });
                        //  Ok(attributes.into_ecs(self.world, self.z_level))
                        //}
                        "barrier" => {
                            let _ = data.barriers.insert(obj_ent, Barrier);
                        }

                        // Otherwise this object was unhandled and should live in the ECS
                        // for something else to pick up.
                        // TODO: Remove Object from components - only use JSON
                        _ => {
                            trace!("object is unknown to TiledSystem:\n{:#?}", obj);
                            let _ = data.objects.insert(obj_ent, obj.clone());
                        }
                    }

                    // Insert the leftover json properties only if there are leftovers and
                    // we didn't already insert an unhandled object into the ECS
                    if !properties.is_empty() {
                        let _ = data.jsons.insert(obj_ent, JSON(properties));
                    }
                }
            }
        }
        z += 1;
    }
}


impl<'s> System<'s> for TiledmapSystem {
    type SystemData = TiledmapSystemData<'s>;

    fn run(&mut self, (entities, mut reqs, mut data): Self::SystemData) {
        // Handle all tiled map load requests by loading the map and then injecting
        // it into the ECS.
        let mut delete = vec![];
        for (ent, LoadMap { file }) in (&entities, &reqs).join() {
            trace!("loading map '{}'", file);
            let res = resources::when_loaded(&mut self.resources, &file, |map| {
                insert_map(map, &mut data);
                delete.push(ent);
            });
            if res.is_err() {
                delete.push(ent);
            }
        }
        delete.into_iter().for_each(|ent| {
            reqs.remove(ent);
        });
    }
}
