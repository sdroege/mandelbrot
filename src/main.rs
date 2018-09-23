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

extern crate futures;
use futures::channel::mpsc as futures_mpsc;
use futures::prelude::*;

use std::sync::mpsc as std_mpsc;

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
struct App {
    view: (f64, f64, f64, f64),
    surface_size: (usize, usize),
    surface: Option<cairo::ImageSurface>,
    selection: Option<((f64, f64), Option<(f64, f64)>)>,
    moving: Option<(f64, f64)>,
    drawing_area: gtk::DrawingArea,
    command_sender: std_mpsc::Sender<Command>,
}

lazy_static! {
    static ref COLORS: [Pixel; 360] = {
        let mut colors = [Default::default(); 360];

        let s = 1.0;
        let v = 1.0;
        for (h, color) in colors.iter_mut().enumerate() {
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

            *color = Pixel::new(
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

    fn interpolate(self, other: Self, frac: f64) -> Self {
        Pixel::new(
            (self.r as f64 + (frac * (other.r as f64 - self.r as f64)))
                .max(0.0)
                .min(255.0) as u8,
            (self.g as f64 + (frac * (other.g as f64 - self.g as f64)))
                .max(0.0)
                .min(255.0) as u8,
            (self.b as f64 + (frac * (other.b as f64 - self.b as f64)))
                .max(0.0)
                .min(255.0) as u8,
        )
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

fn create_image(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    target_width: usize,
    target_height: usize,
) -> cairo::ImageSurface {
    use std::iter;
    let pixels = (0..target_height)
        .map(|target_y| (0..target_width).zip(iter::repeat(target_y)))
        .flatten()
        .map(|(target_x, target_y)| {
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

            if (it as usize) < max_it {
                let log_zn = z.norm_sqr().ln() / 2.0;
                let nu = (log_zn / 2.0f64.ln()).ln() / 2.0f64.ln();

                let it = it as f64 + 1.0 - nu;
                let c1 = COLORS[it.floor() as usize % 360];
                let c2 = COLORS[(it.floor() + 1.0) as usize % 360];
                c1.interpolate(c2, it.fract())
            } else {
                Pixel::default()
            }
        }).collect::<Vec<_>>();

    assert_eq!(pixels.len(), target_width * target_height);
    let pixels = pixels_to_bytes(pixels);

    cairo::ImageSurface::create_for_data(
        pixels,
        cairo::Format::Rgb24,
        target_width as i32,
        target_height as i32,
        (target_width as i32) * 4,
    ).unwrap()
}

#[derive(Debug)]
enum Command {
    Render {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        target_width: usize,
        target_height: usize,
    },
    Quit,
}

fn render_thread(
    commands: &std_mpsc::Receiver<Command>,
    surfaces: &futures_mpsc::UnboundedSender<cairo::ImageSurface>,
) {
    loop {
        let mut command = commands.recv().unwrap();

        // Get last command that was ever send, but always break on quit
        while let Ok(cmd) = commands.try_recv() {
            command = cmd;
            if let Command::Quit = command {
                break;
            }
        }

        match command {
            Command::Quit => break,
            Command::Render {
                x,
                y,
                width,
                height,
                target_width,
                target_height,
            } => {
                let surface = create_image(x, y, width, height, target_width, target_height);
                surfaces.unbounded_send(surface).unwrap();
            }
        }
    }
}

fn calculate_selection_rectangle(
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
    surface_size: (usize, usize),
) -> (f64, f64, f64, f64) {
    let (width, height) = ((x2 - x1), (y2 - y1));
    let (xscale, yscale) = (
        (width / surface_size.0 as f64).abs(),
        (height / surface_size.1 as f64).abs(),
    );
    let (width, height) = if xscale > yscale {
        (
            width,
            height.signum() * (width as f64 * surface_size.1 as f64 / surface_size.0 as f64).abs(),
        )
    } else {
        (
            width.signum() * (height as f64 * surface_size.0 as f64 / surface_size.1 as f64).abs(),
            height,
        )
    };

    (x1, y1, width, height)
}

impl App {
    fn on_draw(&mut self, cr: &cairo::Context) {
        if let Some(ref surface) = self.surface {
            cr.save();
            cr.scale(0.5, 0.5);
            cr.set_operator(cairo::Operator::Source);
            cr.set_source_surface(surface, 0.0, 0.0);
            cr.paint();
            cr.restore();
        } else {
            cr.save();
            cr.set_operator(cairo::Operator::Clear);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.paint();
            cr.restore();
        }

        if let Some(((x1, y1), Some((x2, y2)))) = self.selection {
            let (x, y, width, height) = calculate_selection_rectangle(
                x1 as f64,
                x2 as f64,
                y1 as f64,
                y2 as f64,
                self.surface_size,
            );

            cr.save();
            cr.set_line_width(1.0);
            cr.rectangle(x, y, width, height);
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.stroke();

            cr.rectangle(x, y, width, height);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.2);
            cr.fill();
            cr.restore();
        }
    }

    fn on_motion_notify(&mut self, area: &gtk::DrawingArea, ev: &gdk::EventMotion) {
        if ev.get_state().contains(gdk::ModifierType::BUTTON1_MASK) {
            if let Some(ref mut selection) = &mut self.selection {
                selection.1 = Some(ev.get_position());
                area.queue_draw();
            }
        } else if ev.get_state().contains(gdk::ModifierType::BUTTON3_MASK) {
            let old_view = self.view;
            if let Some(ref mut moving) = &mut self.moving {
                let new_position = ev.get_position();
                let (move_x, move_y) = (
                    ((new_position.0 - moving.0) / self.surface_size.0 as f64) * self.view.2,
                    ((new_position.1 - moving.1) / self.surface_size.1 as f64) * self.view.3,
                );

                self.view.0 -= move_x;
                self.view.1 -= move_y;

                *moving = new_position;
            }

            if old_view != self.view {
                area.queue_draw();
                self.trigger_render();
            }
        }
    }

    fn on_button_press(&mut self, _area: &gtk::DrawingArea, ev: &gdk::EventButton) {
        if ev.get_button() == 1 {
            self.selection = Some((ev.get_position(), None));
        } else if ev.get_button() == 3 {
            self.moving = Some(ev.get_position());
        }
    }

    fn on_button_release(&mut self, area: &gtk::DrawingArea, ev: &gdk::EventButton) {
        if ev.get_button() == 1 {
            let selection = self.selection.take();

            if let Some(((x1, y1), Some((x2, y2)))) = selection {
                let surface_size = self.surface_size;

                let (x, y, width, height) = calculate_selection_rectangle(
                    x1 as f64,
                    x2 as f64,
                    y1 as f64,
                    y2 as f64,
                    surface_size,
                );

                let (x1, x2, y1, y2) = (
                    x.min(x + width),
                    x.max(x + width),
                    y.min(y + height),
                    y.max(y + height),
                );

                let old_view = self.view;
                let view_x1 = old_view.0 + (x1 / surface_size.0 as f64) * old_view.2;
                let view_y1 = old_view.1 + (y1 / surface_size.1 as f64) * old_view.3;
                let view_x2 = old_view.0 + (x2 / surface_size.0 as f64) * old_view.2;
                let view_y2 = old_view.1 + (y2 / surface_size.1 as f64) * old_view.3;

                self.view = (view_x1, view_y1, view_x2 - view_x1, view_y2 - view_y1);

                let _ = self.surface.take();
                area.queue_draw();
                self.trigger_render();
            }
        } else if ev.get_button() == 3 {
            self.moving = None;
        }
    }

    fn on_size_allocate(&mut self, area: &gtk::DrawingArea, allocation: &gdk::Rectangle) {
        let old_size = self.surface_size;
        let new_size = (allocation.width as usize, allocation.height as usize);
        if new_size != old_size {
            if old_size.0 != 0 && old_size.1 != 0 && new_size.0 != 0 && new_size.1 != 0 {
                self.view.2 = self.view.2 * new_size.0 as f64 / old_size.0 as f64;
                self.view.3 = self.view.3 * new_size.1 as f64 / old_size.1 as f64;
            }

            self.surface_size = new_size;
            let _ = self.surface.take();
            area.queue_draw();
            self.trigger_render();
        }
    }

    fn on_render_done(&mut self, surface: cairo::ImageSurface) {
        self.surface = Some(surface);
        self.drawing_area.queue_draw();
    }

    fn trigger_render(&self) {
        self.command_sender
            .send(Command::Render {
                x: self.view.0,
                y: self.view.1,
                width: self.view.2,
                height: self.view.3,
                target_width: self.surface_size.0 * 2,
                target_height: self.surface_size.1 * 2,
            }).unwrap();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.command_sender.send(Command::Quit);
    }
}

fn build_ui(application: &gtk::Application) {
    use std::thread;

    let (command_sender, command_receiver) = std_mpsc::channel();
    let (surface_sender, surface_receiver) = futures_mpsc::unbounded();

    thread::spawn(move || {
        render_thread(&command_receiver, &surface_sender);
    });

    let window = gtk::ApplicationWindow::new(application);
    let area = gtk::DrawingArea::new();
    window.add(&area);

    {
        // See https://github.com/gtk-rs/gtk/issues/704
        use glib::translate::*;
        area.add_events(
            (gdk::EventMask::BUTTON_PRESS_MASK
                | gdk::EventMask::BUTTON_RELEASE_MASK
                | gdk::EventMask::BUTTON1_MOTION_MASK
                | gdk::EventMask::BUTTON3_MOTION_MASK)
                .to_glib() as i32,
        );
    }

    window.set_default_size(800, (800.0 / 1.75) as i32);
    window.set_title("Mandelbrot");

    let view = (-2.5, -1.0, 3.5, 2.0);
    let app = Rc::new(RefCell::new(App {
        view,
        surface_size: (0, 0),
        surface: None,
        selection: None,
        moving: None,
        drawing_area: area.clone(),
        command_sender,
    }));

    let app_weak = Rc::downgrade(&app);
    area.connect_size_allocate(move |area, allocation| {
        if let Some(app) = app_weak.upgrade() {
            app.borrow_mut().on_size_allocate(area, allocation);
        }
    });

    let app_weak = Rc::downgrade(&app);
    area.connect_button_press_event(move |area, ev| {
        if let Some(app) = app_weak.upgrade() {
            app.borrow_mut().on_button_press(area, ev);
        }
        Inhibit(false)
    });

    let app_weak = Rc::downgrade(&app);
    area.connect_button_release_event(move |area, ev| {
        if let Some(app) = app_weak.upgrade() {
            app.borrow_mut().on_button_release(area, ev);
        }
        Inhibit(false)
    });

    let app_weak = Rc::downgrade(&app);
    area.connect_motion_notify_event(move |area, ev| {
        if let Some(app) = app_weak.upgrade() {
            app.borrow_mut().on_motion_notify(area, ev);
        }
        Inhibit(false)
    });

    let app_weak = Rc::downgrade(&app);
    area.connect_draw(move |_, cr| {
        if let Some(app) = app_weak.upgrade() {
            app.borrow_mut().on_draw(cr);
        }
        Inhibit(false)
    });

    let app_weak = Rc::downgrade(&app);
    let main_context = glib::MainContext::default();
    main_context.spawn_local(
        surface_receiver
            .for_each(move |surface| {
                if let Some(app) = app_weak.upgrade() {
                    app.borrow_mut().on_render_done(surface);
                }

                Ok(())
            }).map(|_| ()),
    );

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
