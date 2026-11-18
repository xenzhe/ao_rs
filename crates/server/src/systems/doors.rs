//! Doors: clicking one opens or closes it (`WorldActions.bas::HandleDoorAction`).
//!
//! The door objects come with the map file (`.csm` objects); a door a player toggled is
//! kept in `MapInstance::doors` with its blocks applied to the map, and sent to everyone
//! on the map (and to whoever enters later) as `S2C::MapObject`.
use crate::world::World;
use ao_formats::dat::obj::ObjType;
use ao_protocol::{Tone, S2C};
use ao_rules::doors::{apply_door_blocks, DOOR_CLICK_OFFSETS, DOOR_REACH};

impl World {
    /// The object on a map tile now: the toggled door, or what the map file says.
    pub(crate) fn map_object_at(&mut self, map: u16, x: i32, y: i32) -> Option<u16> {
        if let Some(o) = self.instance(map).doors.get(&(x, y)) {
            return Some(*o);
        }
        self.craft_map_objects(map).iter().find(|o| o.x == x && o.y == y).map(|o| o.obj)
    }

    fn door_at(&mut self, map: u16, x: i32, y: i32) -> Option<(i32, i32, u16)> {
        let ao = self.ao.clone();
        DOOR_CLICK_OFFSETS.iter().find_map(|(dx, dy)| {
            let (tx, ty) = (x + dx, y + dy);
            let obj = self.map_object_at(map, tx, ty)?;
            (ao.objs.get(obj as u32)?.obj_type == ObjType::Doors).then_some((tx, ty, obj))
        })
    }

    /// A click on (x, y): toggles the door there (or the one a multi-tile door covers).
    /// Returns whether there was a door.
    pub(crate) fn door_click(&mut self, id: u32, x: i32, y: i32) -> bool {
        let Some(p) = self.player(id) else { return false };
        let (map, px, py) = (p.map, p.x, p.y);
        let Some((dx, dy, _)) = self.door_at(map, x, y) else { return false };
        self.player_mut(id).target_door = Some((map, dx, dy));
        let dist = (((px - dx).pow(2) + (py - dy).pow(2)) as f64).sqrt();
        if dist > DOOR_REACH as f64 {
            self.console(id, "Estas demasiado lejos.", Tone::Warn);
            return true;
        }
        self.toggle_door(id, map, dx, dy, false);
        true
    }

    fn toggle_door(&mut self, id: u32, map: u16, x: i32, y: i32, by_lever: bool) {
        let ao = self.ao.clone();
        let Some(obj) = self.map_object_at(map, x, y) else { return };
        let Some(d) = ao.objs.get(obj as u32).filter(|d| d.obj_type == ObjType::Doors) else { return };
        if d.key_obj == 1 && !by_lever {
            // Ducts (grh 11444/11445/59877/59878) are opened by a switch somewhere.
            let duct = matches!(d.grh, 11444 | 11445 | 59877 | 59878);
            let msg = if duct { "Al parecer, alguien cerro esta puerta. Debe haber algun interruptor por algun lado..." } else { "La puerta esta cerrada con llave." };
            self.console(id, msg, Tone::Info);
            return;
        }
        let closing = d.locked != 1;
        let new = if closing { d.closed_index } else { d.open_index };
        let Some(nd) = (new > 0).then(|| ao.objs.get(new as u32)).flatten() else { return };
        let new = new as u16;
        // BloquearPuerta reads the subtype of the object now on the tile.
        let sub = nd.subtype;
        let inst = self.maps.get_mut(&map).expect("map loaded");
        apply_door_blocks(&mut inst.map.blocked, x, y, sub, closing);
        inst.doors.insert((x, y), new);
        self.broadcast(map, &S2C::MapObject { x, y, obj: new }, None);
        // A lever (Subtipo 1) works the door three tiles left and one down.
        if sub == 1 && !by_lever {
            self.toggle_door(id, map, x - 3, y + 1, true);
        }
    }

    /// `UseInvItem` for `otKeys` on the last door clicked: with the matching `Clave`, a door
    /// locked with a key is unlocked (it stays closed; the key is spent, `EliminarLlaves`)
    /// and a closed one is locked. Keys under 1000 belong to the keyring (houses), not here.
    pub(crate) fn use_key(&mut self, id: u32, slot: u8) {
        let ao = self.ao.clone();
        let Some(key) = self.player(id).and_then(|p| p.inv.get(slot)).and_then(|it| ao.objs.get(it.obj as u32)) else { return };
        let Some((map, x, y)) = self.player(id).and_then(|p| p.target_door).filter(|(m, ..)| self.player(id).is_some_and(|p| p.map == *m)) else {
            return self.console(id, "Primero hacé clic en la puerta.", Tone::Info);
        };
        let (px, py) = self.player(id).map(|p| (p.x, p.y)).unwrap();
        let dist = ((px - x) * (px - x) + (py - y) * (py - y)) as f64;
        if dist.sqrt() > DOOR_REACH as f64 {
            return self.console(id, "Estas demasiado lejos.", Tone::Warn);
        }
        let Some(obj) = self.map_object_at(map, x, y) else { return };
        let Some(door) = ao.objs.get(obj as u32).filter(|d| d.obj_type == ObjType::Doors) else { return };
        if door.key_code < 1000 {
            return self.console(id, "Las llaves en el inventario están desactivadas. Sólo se permiten en el llavero.", Tone::Warn);
        }
        if door.locked != 1 {
            return self.console(id, "La puerta tiene que estar cerrada.", Tone::Info);
        }
        if door.key_code != key.key_code {
            return self.console(id, "La llave no sirve.", Tone::Warn);
        }
        let unlocking = door.key_obj > 0;
        let (new, msg) = if unlocking { (door.closed_index, "Has abierto la puerta.") } else { (door.locked_index, "Has cerrado con llave la puerta.") };
        if new <= 0 || ao.objs.get(new as u32).is_none() {
            return;
        }
        let new = new as u16;
        self.maps.get_mut(&map).expect("map loaded").doors.insert((x, y), new);
        self.broadcast(map, &S2C::MapObject { x, y, obj: new }, None);
        self.console(id, msg, Tone::Good);
        if unlocking {
            self.remove_from_slot(id, slot, 1);
        }
    }

    /// Doors toggled on this map, for a player who just arrived.
    pub(crate) fn send_doors(&self, id: u32, map: u16) {
        for (&(x, y), &obj) in &self.instance(map).doors {
            self.send(id, &S2C::MapObject { x, y, obj });
        }
    }
}
