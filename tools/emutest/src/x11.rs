//! A private X display: synthetic key presses (XTEST) and screenshots.

use image::RgbImage;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, ConnectionExt as _, ImageFormat};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

pub struct Display {
    conn: RustConnection,
    root: xproto::Window,
    width: u16,
    height: u16,
    keycodes: Vec<(u32, u8)>,
}

/// X keysyms for the keys the emulator's default keyboard profile uses.
pub fn keysym(name: &str) -> Option<u32> {
    Some(match name {
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "enter" => 0xff0d,
        "esc" => 0xff1b,
        s if s.len() == 1 && s.is_ascii() => s.as_bytes()[0].to_ascii_lowercase() as u32,
        _ => return None,
    })
}

impl Display {
    pub fn connect(name: &str) -> Result<Display, String> {
        let (conn, screen) = x11rb::connect(Some(name)).map_err(|e| format!("X display {name}: {e}"))?;
        let s = &conn.setup().roots[screen];
        let (root, width, height) = (s.root, s.width_in_pixels, s.height_in_pixels);
        let (min, max) = (conn.setup().min_keycode, conn.setup().max_keycode);
        let map = conn.get_keyboard_mapping(min, max - min + 1).map_err(|e| e.to_string())?.reply().map_err(|e| e.to_string())?;
        let per = map.keysyms_per_keycode.max(1) as usize;
        let mut keycodes = vec![];
        for (i, syms) in map.keysyms.chunks(per).enumerate() {
            for &k in syms {
                if k != 0 && !keycodes.iter().any(|(s, _)| *s == k) {
                    keycodes.push((k, min + i as u8));
                }
            }
        }
        Ok(Display { conn, root, width, height, keycodes })
    }

    fn sync(&self) {
        let _ = self.conn.flush();
        let _ = self.conn.get_input_focus().ok().and_then(|c| c.reply().ok());
    }

    fn keycode(&self, name: &str) -> Result<u8, String> {
        let sym = keysym(name).ok_or_else(|| format!("unknown key {name}"))?;
        self.keycodes.iter().find(|(s, _)| *s == sym).map(|x| x.1).ok_or_else(|| format!("no keycode for {name}"))
    }

    /// Move the pointer (with no window manager, keyboard focus follows it).
    pub fn pointer(&self, x: i16, y: i16) {
        let _ = self.conn.xtest_fake_input(xproto::MOTION_NOTIFY_EVENT, 0, 0, self.root, x, y, 0);
        self.sync();
    }

    /// Press every key of a `+`-separated combination, hold it, release in reverse order. Games
    /// poll input once a frame, so a tap shorter than a frame or two can be missed.
    pub fn hold(&self, combo: &str, secs: f32) -> Result<(), String> {
        let codes: Vec<u8> = combo.split('+').map(|k| self.keycode(k)).collect::<Result<_, _>>()?;
        for &c in &codes {
            let _ = self.conn.xtest_fake_input(xproto::KEY_PRESS_EVENT, c, 0, self.root, 0, 0, 0);
            self.sync();
        }
        std::thread::sleep(Duration::from_secs_f32(secs));
        for &c in codes.iter().rev() {
            let _ = self.conn.xtest_fake_input(xproto::KEY_RELEASE_EVENT, c, 0, self.root, 0, 0, 0);
            self.sync();
        }
        Ok(())
    }

    pub fn press(&self, key: &str) -> Result<(), String> {
        let c = self.keycode(key)?;
        let _ = self.conn.xtest_fake_input(xproto::KEY_PRESS_EVENT, c, 0, self.root, 0, 0, 0);
        self.sync();
        Ok(())
    }

    pub fn release(&self, key: &str) -> Result<(), String> {
        let c = self.keycode(key)?;
        let _ = self.conn.xtest_fake_input(xproto::KEY_RELEASE_EVENT, c, 0, self.root, 0, 0, 0);
        self.sync();
        Ok(())
    }

    /// The whole screen as RGB (24-bit TrueColor, as Xvfb's default visual).
    pub fn screenshot(&self) -> Result<RgbImage, String> {
        let img = self
            .conn
            .get_image(ImageFormat::Z_PIXMAP, self.root, 0, 0, self.width, self.height, !0)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        let (w, h) = (self.width as u32, self.height as u32);
        if img.data.len() < (w * h * 4) as usize {
            return Err(format!("unexpected image size {} for {w}x{h} (depth {})", img.data.len(), img.depth));
        }
        Ok(RgbImage::from_fn(w, h, |x, y| {
            let i = ((y * w + x) * 4) as usize;
            image::Rgb([img.data[i + 2], img.data[i + 1], img.data[i]])
        }))
    }
}
