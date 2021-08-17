use gio::prelude::*;
use glib::signal::Inhibit;
use gtk4::prelude::*;

use lazy_static::lazy_static;

use num_complex::Complex64;

use rayon::prelude::*;

use std::sync::mpsc;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

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

#[derive(Debug)]
struct Image {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct Rectangle {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug)]
struct App {
    view: Cell<Rectangle>,
    surface_size: Cell<(usize, usize)>,
    surface: RefCell<Option<cairo::ImageSurface>>,
    zoom_controller: gtk4::GestureDrag,
    zoom_controller_cancelled: Cell<bool>,
    move_controller: gtk4::GestureDrag,
    drawing_area: gtk4::DrawingArea,
    command_sender: mpsc::Sender<Command>,
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

fn pixels_to_bytes(pixels: Vec<Pixel>) -> Vec<u8> {
    unsafe {
        use std::mem;

        assert_eq!(4 * mem::size_of::<u8>(), mem::size_of::<Pixel>());

        let mut pixels = mem::ManuallyDrop::new(pixels);
        Vec::from_raw_parts(
            pixels.as_mut_ptr() as *mut u8,
            pixels.len() * 4,
            pixels.capacity() * 4,
        )
    }
}

fn create_image(rect: Rectangle, target_width: usize, target_height: usize) -> Image {
    let (xscale, yscale) = (
        rect.width / (target_width as f64 - 1.0),
        rect.height / (target_height as f64 - 1.0),
    );

    let pixels = (0..target_height)
        .into_par_iter()
        .flat_map(|target_y| rayon::iter::repeatn(target_y, target_width).enumerate())
        .map(|(target_x, target_y)| {
            let c = Complex64::new(
                rect.x + target_x as f64 * xscale,
                rect.y + target_y as f64 * yscale,
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
        })
        .collect::<Vec<_>>();

    assert_eq!(pixels.len(), target_width * target_height);
    let pixels = pixels_to_bytes(pixels);

    Image {
        pixels,
        width: target_width,
        height: target_height,
    }
}

#[derive(Debug)]
enum Command {
    Render {
        rect: Rectangle,
        target_width: usize,
        target_height: usize,
    },
    Quit,
}

fn render_thread(commands: &mpsc::Receiver<Command>, surfaces: &glib::Sender<Image>) {
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
                rect,
                target_width,
                target_height,
            } => {
                let surface = create_image(rect, target_width, target_height);
                surfaces.send(surface).unwrap();
            }
        }
    }
}

fn calculate_selection_rectangle(rect: Rectangle, surface_size: (usize, usize)) -> Rectangle {
    let (xscale, yscale) = (
        (rect.width / surface_size.0 as f64).abs(),
        (rect.height / surface_size.1 as f64).abs(),
    );

    let (width, height) = if xscale > yscale {
        (
            rect.width,
            rect.height.signum()
                * (rect.width as f64 * surface_size.1 as f64 / surface_size.0 as f64).abs(),
        )
    } else {
        (
            rect.width.signum()
                * (rect.height as f64 * surface_size.0 as f64 / surface_size.1 as f64).abs(),
            rect.height,
        )
    };

    Rectangle {
        width,
        height,
        ..rect
    }
}

impl App {
    fn on_draw(&self, cr: &cairo::Context) {
        if let Some(ref surface) = *self.surface.borrow() {
            cr.save().expect("Failed to save cairo state");
            cr.scale(0.5, 0.5);
            cr.set_operator(cairo::Operator::Source);
            cr.set_source_surface(surface, 0.0, 0.0)
                .expect("Failed to set source surface");
            cr.paint().expect("Failed to paint");
            cr.restore().expect("Failed to restore cairo state");
        } else {
            cr.save().expect("Failed to save cairo state");
            cr.set_operator(cairo::Operator::Clear);
            cr.set_source_rgb(0.0, 0.0, 0.0);
            cr.paint().expect("Failed to paint");
            cr.restore().expect("Failed to restore cairo state");
        }

        if self.zoom_controller.is_recognized() {
            if let (Some((x, y)), Some((width, height))) = (
                self.zoom_controller.start_point(),
                self.zoom_controller.offset(),
            ) {
                let rect = Rectangle {
                    x,
                    y,
                    width,
                    height,
                };
                let rect = calculate_selection_rectangle(rect, self.surface_size.get());

                cr.save().expect("Failed to save cairo state");
                cr.set_line_width(1.0);
                cr.rectangle(rect.x, rect.y, rect.width, rect.height);
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.stroke().expect("Failed to stroke");

                cr.rectangle(rect.x, rect.y, rect.width, rect.height);
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.2);
                cr.fill().expect("Failed to fill");
                cr.restore().expect("Failed to restore cairo state");
            }
        }
    }

    fn on_zoom_begin(&self, _controller: &gtk4::GestureDrag, _x: f64, _y: f64) {
        self.zoom_controller_cancelled.set(false);
        self.move_controller.reset();
    }

    fn on_zoom_update(&self, _controller: &gtk4::GestureDrag, _off_x: f64, _off_y: f64) {
        self.drawing_area.queue_draw();
    }

    fn on_zoom_end(&self, _controller: &gtk4::GestureDrag, _off_x: f64, _off_y: f64) {
        if self.zoom_controller_cancelled.get() {
            return;
        }

        if let (Some((x, y)), Some((width, height))) = (
            self.zoom_controller.start_point(),
            self.zoom_controller.offset(),
        ) {
            let rect = Rectangle {
                x,
                y,
                width,
                height,
            };

            let rect = calculate_selection_rectangle(rect, self.surface_size.get());

            let (x1, x2, y1, y2) = (
                f64::min(rect.x, rect.x + rect.width),
                f64::max(rect.x, rect.x + rect.width),
                f64::min(rect.y, rect.y + rect.height),
                f64::max(rect.y, rect.y + rect.height),
            );

            let view = self.view.get();
            let surface_size = self.surface_size.get();
            let (xscale, yscale) = (
                view.width / surface_size.0 as f64,
                view.height / surface_size.1 as f64,
            );

            self.view.set(Rectangle {
                x: view.x + x1 * xscale,
                y: view.y + y1 * yscale,
                width: (x2 - x1) * xscale,
                height: (y2 - y1) * yscale,
            });

            let _ = self.surface.borrow_mut().take();
            self.trigger_render();
        }

        self.drawing_area.queue_draw();
    }

    fn on_zoom_cancelled(&self, _controller: &gtk4::GestureDrag) {
        self.zoom_controller_cancelled.set(true);
    }

    fn on_move_begin(&self, _controller: &gtk4::GestureDrag, _x: f64, _y: f64) {
        self.zoom_controller.reset();
    }

    fn on_move_update(&self, _controller: &gtk4::GestureDrag, _off_x: f64, _off_y: f64) {
        self.drawing_area.queue_draw();
        self.trigger_render();
    }

    fn on_move_end(&self, _controller: &gtk4::GestureDrag, _off_x: f64, _off_y: f64) {
        if let Some((x, y)) = self.move_controller.offset() {
            let mut view = self.view.get();
            let surface_size = self.surface_size.get();
            view.x -= x / surface_size.0 as f64 * view.width;
            view.y -= y / surface_size.1 as f64 * view.height;
            self.view.set(view);

            self.drawing_area.queue_draw();
            self.trigger_render();
        }
    }

    fn on_key_pressed(&self, keyval: gdk4::keys::Key, _keycode: u32, _state: gdk4::ModifierType) {
        if keyval == gdk4::keys::constants::Escape {
            self.zoom_controller.reset();
            self.drawing_area.queue_draw();
        }
    }

    fn on_resize(&self, area: &gtk4::DrawingArea, width: i32, height: i32) {
        let old_size = self.surface_size.get();
        let new_size = (width as usize, height as usize);
        if new_size != old_size {
            if old_size.0 != 0 && old_size.1 != 0 && new_size.0 != 0 && new_size.1 != 0 {
                let mut view = self.view.get();
                view.width *= new_size.0 as f64 / old_size.0 as f64;
                view.height *= new_size.1 as f64 / old_size.1 as f64;
                self.view.set(view);
            }

            self.surface_size.set(new_size);
            area.queue_draw();
            self.trigger_render();
        }
    }

    fn on_render_done(&self, surface: cairo::ImageSurface) {
        *self.surface.borrow_mut() = Some(surface);
        self.drawing_area.queue_draw();
    }

    fn trigger_render(&self) {
        let mut rect = self.view.get();
        let surface_size = self.surface_size.get();

        if self.move_controller.is_recognized() {
            if let Some((x, y)) = self.move_controller.offset() {
                let view = self.view.get();
                rect.x -= x / surface_size.0 as f64 * view.width;
                rect.y -= y / surface_size.1 as f64 * view.height;
            }
        }

        self.command_sender
            .send(Command::Render {
                rect,
                target_width: surface_size.0 * 2,
                target_height: surface_size.1 * 2,
            })
            .unwrap();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.command_sender.send(Command::Quit);
    }
}

fn build_ui(application: &gtk4::Application) {
    use std::thread;

    let (command_sender, command_receiver) = mpsc::channel();
    let (surface_sender, surface_receiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

    thread::spawn(move || {
        render_thread(&command_receiver, &surface_sender);
    });

    let window = gtk4::ApplicationWindow::new(application);
    let area = gtk4::DrawingArea::new();
    window.set_child(Some(&area));

    window.set_default_size(800, (800.0 / 1.75) as i32);
    window.set_title(Some("Mandelbrot"));

    let view = Rectangle {
        x: -2.5,
        y: -1.0,
        width: 3.5,
        height: 2.0,
    };

    let zoom_controller = gtk4::GestureDrag::new();
    zoom_controller.set_button(1);
    let move_controller = gtk4::GestureDrag::new();
    move_controller.set_button(3);

    let app = Rc::new(App {
        view: Cell::new(view),
        surface_size: Cell::new((0, 0)),
        surface: RefCell::new(None),
        zoom_controller: zoom_controller.clone(),
        zoom_controller_cancelled: Cell::new(false),
        move_controller: move_controller.clone(),
        drawing_area: area.clone(),
        command_sender,
    });

    let app_weak = Rc::downgrade(&app);
    area.connect_resize(move |area, width, height| {
        if let Some(app) = app_weak.upgrade() {
            app.on_resize(area, width, height);
        }
    });

    area.add_controller(&zoom_controller);
    let app_weak = Rc::downgrade(&app);
    zoom_controller.connect_drag_begin(move |controller, x, y| {
        if let Some(app) = app_weak.upgrade() {
            app.on_zoom_begin(controller, x, y);
        }
    });

    let app_weak = Rc::downgrade(&app);
    zoom_controller.connect_drag_update(move |controller, off_x, off_y| {
        if let Some(app) = app_weak.upgrade() {
            app.on_zoom_update(controller, off_x, off_y);
        }
    });

    let app_weak = Rc::downgrade(&app);
    zoom_controller.connect_cancel(move |controller, _sequence| {
        if let Some(app) = app_weak.upgrade() {
            app.on_zoom_cancelled(controller);
        }
    });

    let app_weak = Rc::downgrade(&app);
    zoom_controller.connect_drag_end(move |controller, off_x, off_y| {
        if let Some(app) = app_weak.upgrade() {
            app.on_zoom_end(controller, off_x, off_y);
        }
    });

    area.add_controller(&move_controller);
    let app_weak = Rc::downgrade(&app);
    move_controller.connect_drag_begin(move |controller, x, y| {
        if let Some(app) = app_weak.upgrade() {
            app.on_move_begin(controller, x, y);
        }
    });

    let app_weak = Rc::downgrade(&app);
    move_controller.connect_drag_update(move |controller, off_x, off_y| {
        if let Some(app) = app_weak.upgrade() {
            app.on_move_update(controller, off_x, off_y);
        }
    });

    let app_weak = Rc::downgrade(&app);
    move_controller.connect_drag_end(move |controller, off_x, off_y| {
        if let Some(app) = app_weak.upgrade() {
            app.on_move_end(controller, off_x, off_y);
        }
    });

    let app_weak = Rc::downgrade(&app);
    area.set_draw_func(move |_, cr, _width, _height| {
        if let Some(app) = app_weak.upgrade() {
            app.on_draw(cr);
        }
    });

    let key_controller = gtk4::EventControllerKey::new();
    window.add_controller(&key_controller);
    let app_weak = Rc::downgrade(&app);
    key_controller.connect_key_pressed(move |_, keyval, keycode, state| {
        if let Some(app) = app_weak.upgrade() {
            app.on_key_pressed(keyval, keycode, state);
        }
        Inhibit(false)
    });

    let app_weak = Rc::downgrade(&app);
    let main_context = glib::MainContext::default();
    surface_receiver.attach(Some(&main_context), move |image| {
        if let Some(app) = app_weak.upgrade() {
            let surface = cairo::ImageSurface::create_for_data(
                image.pixels,
                cairo::Format::Rgb24,
                image.width as i32,
                image.height as i32,
                image.width as i32 * 4,
            )
            .unwrap();
            app.on_render_done(surface);
        }

        glib::Continue(true)
    });

    let app = RefCell::new(Some(app));
    window.connect_close_request(move |win| {
        let _ = app.borrow_mut().take();
        win.close();
        Inhibit(false)
    });

    window.show();
}

fn main() {
    let application = gtk4::Application::new(
        Some("net.coaxion.mandelbrot"),
        gio::ApplicationFlags::empty(),
    );

    application.connect_startup(|app| {
        build_ui(app);
    });
    application.connect_activate(|_| {});
    application.run();
}
