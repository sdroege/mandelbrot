extern crate glib;
use glib::prelude::*;

extern crate gio;
use gio::prelude::*;

extern crate gdk;
use gdk::prelude::*;

extern crate gtk;
use gtk::prelude::*;

extern crate cairo;
use cairo::prelude::*;

#[macro_use]
extern crate lazy_static;

extern crate num_complex;
use num_complex::Complex64;

use std::cell::RefCell;
use std::rc::Rc;

use std::env;

#[cfg(target_endian = "big")]
#[repr(packed)]
#[derive(Default, Clone, Copy, Debug)]
struct Pixel {
    x: u8,
    r: u8,
    g: u8,
    b: u8,
}
#[cfg(target_endian = "little")]
#[repr(packed)]
#[derive(Default, Clone, Copy, Debug)]
struct Pixel {
    b: u8,
    g: u8,
    r: u8,
    x: u8,
}

lazy_static! {
    static ref COLORS: [Pixel; 360] = {
        let mut colors = [Default::default(); 360];

        let s = 1.0;
        let v = 1.0;
        for h in 0..360 {
            let c = v * s;
            let x = c * (1.0 - (((h as f64) / 60.0) % 2.0 - 1.0).abs());
            let m = v - c;

            let (r, g, b) = if h < 60 {
                (c, x, 0.0)
            } else if h < 120 {
                (x, c, 0.0)
            } else if h < 180 {
                (0.0, c, x)
            } else if h < 240 {
                (0.0, x, c)
            } else if h < 300 {
                (x, 0.0, c)
            } else {
                (c, 0.0, x)
            };

            colors[h] = Pixel::new(
                ((r + m) * 255.0) as u8,
                ((g + m) * 255.0) as u8,
                ((b + m) * 255.0) as u8,
            );
        }

        colors
    };
}

impl Pixel {
    fn new(r: u8, g: u8, b: u8) -> Self {
        Pixel { x: 0, r, g, b }
    }
}

fn pixels_to_bytes(mut pixels: Vec<Pixel>) -> Vec<u8> {
    unsafe {
        use std::mem;

        let new_pixels = Vec::from_raw_parts(
            pixels.as_mut_ptr() as *mut u8,
            pixels.len() * 4,
            pixels.capacity() * 4,
        );
        mem::forget(pixels);

        new_pixels
    }
}

struct App {
    view: RefCell<(f64, f64, f64, f64)>,
    surface_size: RefCell<(usize, usize)>,
    surface: RefCell<Option<cairo::ImageSurface>>,
    selection: RefCell<Option<((f64, f64), Option<(f64, f64)>)>>,
}

fn create_image(x: f64, y: f64, width: f64, height: f64, target_width: usize, target_height: usize) -> cairo::ImageSurface {
    let mut pixels: Vec<Pixel> = Vec::with_capacity(target_width * target_height);
    unsafe {
        pixels.set_len(target_width * target_height);
    }

    for target_y in 0..target_height {
        for target_x in 0..target_width {
            let c = Complex64::new(
                x + (target_x as f64 / (target_width as f64 - 1.0)) * width,
                y + (target_y as f64 / (target_height as f64 - 1.0)) * height,
            );

            let mut z = Complex64::new(0.0, 0.0);
            let mut it = 0;
            let max_it = 1000;

            while z.norm_sqr() < ((1 << 16) as f64) && it < max_it {
                z = z * z + c;
                it += 1;
            }

            let c = if (it as usize) < max_it {
                let log_zn = z.norm_sqr().ln() / 2.0;
                let nu = (log_zn / 2.0f64.ln()).ln() / 2.0f64.ln();

                let it = it as f64 + 1.0 - nu;
                let c1 = COLORS[it.floor() as usize % 360];
                let c2 = COLORS[(it.floor() + 1.0) as usize % 360];
                Pixel::new(
                    (c1.r as f64 + (it.fract() * (c2.r as f64 - c1.r as f64)))
                        .max(0.0)
                        .min(255.0) as u8,
                    (c1.g as f64 + (it.fract() * (c2.g as f64 - c1.g as f64)))
                        .max(0.0)
                        .min(255.0) as u8,
                    (c1.b as f64 + (it.fract() * (c2.b as f64 - c1.b as f64)))
                        .max(0.0)
                        .min(255.0) as u8,
                )
            } else {
                Pixel::default()
            };

            unsafe {
                *pixels.get_unchecked_mut(target_y * target_width + target_x) = c;
            }
        }
    }
    assert_eq!(pixels.len(), target_width * target_height);
    let pixels = pixels_to_bytes(pixels);

    let surface = cairo::ImageSurface::create_for_data(
        pixels,
        cairo::Format::Rgb24,
        target_width as i32,
        target_height as i32,
        (target_width as i32) * 4,
    ).unwrap();

    surface
}

fn build_ui(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);
    let area = gtk::DrawingArea::new();
    window.add(&area);

    {
        // See https://github.com/gtk-rs/gtk/issues/704
        use glib::translate::*;
        area.add_events(
            (gdk::EventMask::BUTTON_PRESS_MASK
                | gdk::EventMask::BUTTON_RELEASE_MASK
                | gdk::EventMask::BUTTON1_MOTION_MASK)
                .to_glib() as i32,
        );
    }

    window.set_default_size(800, (800.0 / 1.75) as i32);
    window.set_title("Mandelbrot");

    let view = (-2.5, -1.0, 3.5, 2.0);
    let app = Rc::new(App {
        view: RefCell::new(view),
        surface_size: RefCell::new((0, 0)),
        surface: RefCell::new(None),
        selection: RefCell::new(None),
    });

    let app_weak = Rc::downgrade(&app);
    area.connect_size_allocate(move |area, allocation| {
        let app = match app_weak.upgrade() {
            None => return,
            Some(app) => app,
        };

        let old_size = *app.surface_size.borrow();
        let new_size = (allocation.width as usize, allocation.height as usize);
        if new_size != old_size {
            let mut view = app.view.borrow_mut();

            if old_size.0 != 0 && old_size.1 != 0 && new_size.0 != 0 && new_size.1 != 0 {
                view.2 = view.2 as f64 * new_size.0 as f64 / old_size.0 as f64;
                view.3 = view.3 as f64 * new_size.1 as f64 / old_size.1 as f64;
            }

            *app.surface_size.borrow_mut() = new_size;
            let _ = app.surface.borrow_mut().take();
            area.queue_draw();
        }
    });

    let app_weak = Rc::downgrade(&app);
    area.connect_button_press_event(move |_, ev| {
        let app = match app_weak.upgrade() {
            None => return Inhibit(false),
            Some(app) => app,
        };

        if ev.get_button() == 1 {
            *app.selection.borrow_mut() = Some((ev.get_position(), None));
        }

        Inhibit(false)
    });

    let app_weak = Rc::downgrade(&app);
    area.connect_button_release_event(move |area, ev| {
        let app = match app_weak.upgrade() {
            None => return Inhibit(false),
            Some(app) => app,
        };

        if ev.get_button() == 1 {
            let selection = app.selection.borrow_mut().take();


            if let Some(((x1, y1), Some((x2, y2)))) = selection {
                let surface_size = *app.surface_size.borrow();
                let (x1, x2, y1, y2) = (x1 as f64, x2 as f64, y1 as f64, y2 as f64);
                let width = x2 - x1;
                let height = y2 - y1;

                let (width, height) = if width > height {
                    (width, width as f64 * surface_size.1 as f64 / surface_size.0 as f64)
                } else {
                    (height as f64 * surface_size.0 as f64 / surface_size.1 as f64, height)
                };

                let (x1, x2, y1, y2) = (
                    x1.min(x1 + width),
                    x1.max(x1 + width),
                    y1.min(y1 + height),
                    y1.max(y1 + height),
                );

                let old_view = *app.view.borrow();
                let view_x1 = old_view.0 + (x1 / surface_size.0 as f64) * old_view.2;
                let view_y1 = old_view.1 + (y1 / surface_size.1 as f64) * old_view.3;
                let view_x2 = old_view.0 + (x2 / surface_size.0 as f64) * old_view.2;
                let view_y2 = old_view.1 + (y2 / surface_size.1 as f64) * old_view.3;

                *app.view.borrow_mut() = (
                    view_x1, view_y1,
                    view_x2 - view_x1,
                    view_y2 - view_y1,
                );

                let _ = app.surface.borrow_mut().take();
                area.queue_draw();
            }
        }

        Inhibit(false)
    });

    let app_weak = Rc::downgrade(&app);
    area.connect_motion_notify_event(move |area, ev| {
        let app = match app_weak.upgrade() {
            None => return Inhibit(false),
            Some(app) => app,
        };

        if ev.get_state().contains(gdk::ModifierType::BUTTON1_MASK) {
            if let Some(ref mut selection) = &mut *app.selection.borrow_mut() {
                *selection = (selection.0, Some(ev.get_position()));
                area.queue_draw();
            }
        }

        Inhibit(false)
    });

    let app_weak = Rc::downgrade(&app);
    area.connect_draw(move |_, cr| {
        let app = match app_weak.upgrade() {
            None => return Inhibit(false),
            Some(app) => app,
        };

        let surface_size = app.surface_size.borrow();
        if app.surface.borrow().is_none() {
            let view = app.view.borrow();
            *app.surface.borrow_mut() = Some(create_image(view.0, view.1, view.2, view.3, surface_size.0 * 2, surface_size.1 * 2));
        }

        if let Some(ref surface) = app.surface.borrow().as_ref() {
            cr.save();
            cr.scale(0.5, 0.5);
            cr.set_source_surface(surface, 0.0, 0.0);
            cr.paint();
            cr.restore();
        }

        if let Some(((x1, y1), Some((x2, y2)))) = *app.selection.borrow() {
            let (x1, x2, y1, y2) = (x1 as f64, x2 as f64, y1 as f64, y2 as f64);
            let width = x2 - x1;
            let height = y2 - y1;

            let (width, height) = if width > height {
                (width, width as f64 * surface_size.1 as f64 / surface_size.0 as f64)
            } else {
                (height as f64 * surface_size.0 as f64 / surface_size.1 as f64, height)
            };

            cr.rectangle(x1, y1, width, height);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.stroke();
        }

        Inhibit(false)
    });

    let app = RefCell::new(Some(app));
    window.connect_delete_event(move |win, _| {
        let _ = app.borrow_mut().take();
        win.destroy();
        Inhibit(false)
    });

    window.show_all();
}

fn main() {
    let application =
        gtk::Application::new("net.coaxion.mandelbrot", gio::ApplicationFlags::empty())
            .expect("Initialization failed...");

    application.connect_startup(|app| {
        build_ui(app);
    });
    application.connect_activate(|_| {});
    application.run(&env::args().collect::<Vec<_>>());
}
