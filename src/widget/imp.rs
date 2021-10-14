use gtk::graphene;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use num_complex::Complex64;

use rayon::prelude::*;

use std::sync::mpsc;

use std::cell::{Cell, RefCell};

use lazy_static::lazy_static;

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
    pixels: Vec<Pixel>,
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
enum Command {
    Render {
        rect: Rectangle,
        target_width: usize,
        target_height: usize,
    },
    Quit,
}

#[derive(Debug)]
pub struct Widget {
    view: Cell<Rectangle>,
    surface_size: Cell<(usize, usize)>,
    surface: RefCell<Option<cairo::ImageSurface>>,
    zoom_controller: gtk::GestureDrag,
    zoom_controller_cancelled: Cell<bool>,
    move_controller: gtk::GestureDrag,
    command_sender: mpsc::Sender<Command>,
    surface_receiver: RefCell<Option<glib::Receiver<Image>>>,
    channel_source: RefCell<Option<glib::Source>>,
}

impl Default for Widget {
    fn default() -> Self {
        use std::thread;

        let (command_sender, command_receiver) = mpsc::channel();
        let (surface_sender, surface_receiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

        thread::spawn(move || {
            render_thread(&command_receiver, &surface_sender);
        });

        let view = Rectangle {
            x: -2.5,
            y: -1.0,
            width: 3.5,
            height: 2.0,
        };

        let zoom_controller = gtk::GestureDrag::new();
        zoom_controller.set_button(1);
        let move_controller = gtk::GestureDrag::new();
        move_controller.set_button(3);

        Widget {
            view: Cell::new(view),
            surface_size: Cell::new((0, 0)),
            surface: RefCell::new(None),
            zoom_controller,
            zoom_controller_cancelled: Cell::new(false),
            move_controller,
            command_sender,
            surface_receiver: RefCell::new(Some(surface_receiver)),
            channel_source: RefCell::new(None),
        }
    }
}

impl Drop for Widget {
    fn drop(&mut self) {
        let _ = self.command_sender.send(Command::Quit);
        if let Some(source) = self.channel_source.borrow_mut().take() {
            source.destroy();
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for Widget {
    const NAME: &'static str = "Widget";
    type Type = super::Widget;
    type ParentType = gtk::Widget;
}

impl ObjectImpl for Widget {
    fn constructed(&self, widget: &Self::Type) {
        self.parent_constructed(widget);

        widget.set_focusable(true);

        widget.add_controller(&self.zoom_controller);

        let widget_weak = widget.downgrade();
        self.zoom_controller
            .connect_drag_begin(move |controller, x, y| {
                if let Some(widget) = widget_weak.upgrade() {
                    let imp = Widget::from_instance(&widget);
                    imp.on_zoom_begin(&widget, controller, x, y);
                }
            });

        let widget_weak = widget.downgrade();
        self.zoom_controller
            .connect_drag_update(move |controller, off_x, off_y| {
                if let Some(widget) = widget_weak.upgrade() {
                    let imp = Widget::from_instance(&widget);
                    imp.on_zoom_update(&widget, controller, off_x, off_y);
                }
            });

        let widget_weak = widget.downgrade();
        self.zoom_controller
            .connect_cancel(move |controller, _sequence| {
                if let Some(widget) = widget_weak.upgrade() {
                    let imp = Widget::from_instance(&widget);
                    imp.on_zoom_cancelled(&widget, controller);
                }
            });

        let widget_weak = widget.downgrade();
        self.zoom_controller
            .connect_drag_end(move |controller, off_x, off_y| {
                if let Some(widget) = widget_weak.upgrade() {
                    let imp = Widget::from_instance(&widget);
                    imp.on_zoom_end(&widget, controller, off_x, off_y);
                }
            });

        widget.add_controller(&self.move_controller);
        let widget_weak = widget.downgrade();
        self.move_controller
            .connect_drag_begin(move |controller, x, y| {
                if let Some(widget) = widget_weak.upgrade() {
                    let imp = Widget::from_instance(&widget);
                    imp.on_move_begin(&widget, controller, x, y);
                }
            });

        let widget_weak = widget.downgrade();
        self.move_controller
            .connect_drag_update(move |controller, off_x, off_y| {
                if let Some(widget) = widget_weak.upgrade() {
                    let imp = Widget::from_instance(&widget);
                    imp.on_move_update(&widget, controller, off_x, off_y);
                }
            });

        let widget_weak = widget.downgrade();
        self.move_controller
            .connect_drag_end(move |controller, off_x, off_y| {
                if let Some(widget) = widget_weak.upgrade() {
                    let imp = Widget::from_instance(&widget);
                    imp.on_move_end(&widget, controller, off_x, off_y);
                }
            });

        let key_controller = gtk::EventControllerKey::new();
        widget.add_controller(&key_controller);
        let widget_weak = widget.downgrade();
        key_controller.connect_key_pressed(move |_, keyval, keycode, state| {
            if let Some(widget) = widget_weak.upgrade() {
                let imp = Widget::from_instance(&widget);
                imp.on_key_pressed(&widget, keyval, keycode, state);
            }
            gtk::Inhibit(false)
        });

        let widget_weak = widget.downgrade();
        let main_context = glib::MainContext::default();
        let source_id = self.surface_receiver.borrow_mut().take().unwrap().attach(
            Some(&main_context),
            move |image| {
                let widget = match widget_weak.upgrade() {
                    Some(widget) => widget,
                    None => return glib::Continue(false),
                };

                let imp = Widget::from_instance(&widget);

                let (width, height, stride) = (
                    image.width as i32,
                    image.height as i32,
                    image.width as i32 * 4,
                );
                let surface = cairo::ImageSurface::create_for_data(
                    image,
                    cairo::Format::Rgb24,
                    width,
                    height,
                    stride,
                )
                .unwrap();
                imp.on_render_done(&widget, surface);

                glib::Continue(true)
            },
        );

        let source = main_context
            .find_source_by_id(&source_id)
            .expect("Source not found");
        *self.channel_source.borrow_mut() = Some(source);
    }
}

impl Widget {
    fn on_resize(&self, widget: &super::Widget, width: i32, height: i32) {
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
            widget.queue_draw();
            self.trigger_render(&widget);
        }
    }

    fn on_draw(&self, _widget: &super::Widget, cr: &cairo::Context) {
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

    fn on_zoom_begin(
        &self,
        _widget: &super::Widget,
        _controller: &gtk::GestureDrag,
        _x: f64,
        _y: f64,
    ) {
        self.zoom_controller_cancelled.set(false);
        self.move_controller.reset();
    }

    fn on_zoom_update(
        &self,
        widget: &super::Widget,
        _controller: &gtk::GestureDrag,
        _off_x: f64,
        _off_y: f64,
    ) {
        widget.queue_draw();
    }

    fn on_zoom_end(
        &self,
        widget: &super::Widget,
        _controller: &gtk::GestureDrag,
        _off_x: f64,
        _off_y: f64,
    ) {
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
            self.trigger_render(widget);
        }

        widget.queue_draw();
    }

    fn on_zoom_cancelled(&self, _widget: &super::Widget, _controller: &gtk::GestureDrag) {
        self.zoom_controller_cancelled.set(true);
    }

    fn on_move_begin(
        &self,
        _widget: &super::Widget,
        _controller: &gtk::GestureDrag,
        _x: f64,
        _y: f64,
    ) {
        self.zoom_controller.reset();
    }

    fn on_move_update(
        &self,
        widget: &super::Widget,
        _controller: &gtk::GestureDrag,
        _off_x: f64,
        _off_y: f64,
    ) {
        widget.queue_draw();
        self.trigger_render(widget);
    }

    fn on_move_end(
        &self,
        widget: &super::Widget,
        _controller: &gtk::GestureDrag,
        _off_x: f64,
        _off_y: f64,
    ) {
        if let Some((x, y)) = self.move_controller.offset() {
            let mut view = self.view.get();
            let surface_size = self.surface_size.get();
            view.x -= x / surface_size.0 as f64 * view.width;
            view.y -= y / surface_size.1 as f64 * view.height;
            self.view.set(view);

            widget.queue_draw();
            self.trigger_render(widget);
        }
    }

    fn on_key_pressed(
        &self,
        widget: &super::Widget,
        keyval: gdk::keys::Key,
        _keycode: u32,
        _state: gdk::ModifierType,
    ) {
        if keyval == gdk::keys::constants::Escape {
            self.zoom_controller.reset();
            widget.queue_draw();
        }
    }

    fn on_render_done(&self, widget: &super::Widget, surface: cairo::ImageSurface) {
        *self.surface.borrow_mut() = Some(surface);
        widget.queue_draw();
    }

    fn trigger_render(&self, _widget: &super::Widget) {
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

impl WidgetImpl for Widget {
    fn measure(
        &self,
        _widget: &Self::Type,
        _orientation: gtk::Orientation,
        _for_size: i32,
    ) -> (i32, i32, i32, i32) {
        (0, 0, -1, -1)
    }

    fn size_allocate(&self, widget: &Self::Type, width: i32, height: i32, _baseline: i32) {
        self.on_resize(widget, width, height);
    }

    fn snapshot(&self, widget: &Self::Type, snapshot: &gtk::Snapshot) {
        let surface_size = self.surface_size.get();
        let cr = snapshot
            .append_cairo(&graphene::Rect::new(
                0.0,
                0.0,
                surface_size.0 as f32,
                surface_size.1 as f32,
            ))
            .unwrap();

        self.on_draw(widget, &cr);
    }
}

impl AsRef<[u8]> for Image {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            use std::slice;

            let (ptr, len) = (self.pixels.as_ptr(), self.pixels.len());
            slice::from_raw_parts(ptr as *const u8, len * 4)
        }
    }
}

impl AsMut<[u8]> for Image {
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe {
            use std::slice;

            let (ptr, len) = (self.pixels.as_mut_ptr(), self.pixels.len());
            slice::from_raw_parts_mut(ptr as *mut u8, len * 4)
        }
    }
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

    Image {
        pixels,
        width: target_width,
        height: target_height,
    }
}
