use gtk::{gdk, glib, graphene, gsk, prelude::*, subclass::prelude::*};

use num_complex::Complex64;

use rayon::prelude::*;

use std::cell::{Cell, RefCell};
use std::sync::mpsc;

use once_cell::sync::Lazy;

#[cfg(target_endian = "big")]
#[repr(packed)]
#[derive(Clone, Copy, Debug)]
struct Pixel {
    b: u8,
    g: u8,
    r: u8,
    a: u8,
}
#[cfg(target_endian = "little")]
#[repr(packed)]
#[derive(Clone, Copy, Debug)]
struct Pixel {
    #[allow(dead_code)]
    a: u8,
    r: u8,
    g: u8,
    b: u8,
}

impl Default for Pixel {
    fn default() -> Self {
        Pixel {
            a: 255,
            r: 0,
            g: 0,
            b: 0,
        }
    }
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
    texture: RefCell<Option<gdk::MemoryTexture>>,
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
            texture: RefCell::new(None),
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
    fn constructed(&self) {
        self.parent_constructed();

        let instance = self.instance();

        instance.set_focusable(true);

        instance.add_controller(&self.zoom_controller);

        self.zoom_controller
            .connect_drag_begin(move |controller, x, y| {
                let widget = controller.widget().downcast::<super::Widget>().unwrap();
                let imp = widget.imp();
                imp.on_zoom_begin(controller, x, y);
            });

        self.zoom_controller
            .connect_drag_update(move |controller, off_x, off_y| {
                let widget = controller.widget().downcast::<super::Widget>().unwrap();
                let imp = widget.imp();
                imp.on_zoom_update(controller, off_x, off_y);
            });

        self.zoom_controller
            .connect_cancel(move |controller, _sequence| {
                let widget = controller.widget().downcast::<super::Widget>().unwrap();
                let imp = widget.imp();
                imp.on_zoom_cancelled(controller);
            });

        self.zoom_controller
            .connect_drag_end(move |controller, off_x, off_y| {
                let widget = controller.widget().downcast::<super::Widget>().unwrap();
                let imp = widget.imp();
                imp.on_zoom_end(controller, off_x, off_y);
            });

        instance.add_controller(&self.move_controller);

        self.move_controller
            .connect_drag_begin(move |controller, x, y| {
                let widget = controller.widget().downcast::<super::Widget>().unwrap();
                let imp = widget.imp();
                imp.on_move_begin(controller, x, y);
            });

        self.move_controller
            .connect_drag_update(move |controller, off_x, off_y| {
                let widget = controller.widget().downcast::<super::Widget>().unwrap();
                let imp = widget.imp();
                imp.on_move_update(controller, off_x, off_y);
            });

        self.move_controller
            .connect_drag_end(move |controller, off_x, off_y| {
                let widget = controller.widget().downcast::<super::Widget>().unwrap();
                let imp = widget.imp();
                imp.on_move_end(controller, off_x, off_y);
            });

        let key_controller = gtk::EventControllerKey::new();
        instance.add_controller(&key_controller);

        key_controller.connect_key_pressed(move |controller, keyval, keycode, state| {
            let widget = controller.widget().downcast::<super::Widget>().unwrap();
            let imp = widget.imp();
            imp.on_key_pressed(keyval, keycode, state);
            gtk::Inhibit(false)
        });

        let imp_weak = self.downgrade();
        let main_context = glib::MainContext::default();
        let source_id = self.surface_receiver.borrow_mut().take().unwrap().attach(
            Some(&main_context),
            move |image| {
                let imp = match imp_weak.upgrade() {
                    Some(imp) => imp,
                    None => return glib::Continue(false),
                };

                imp.on_render_done(image);

                glib::Continue(true)
            },
        );

        let source = main_context
            .find_source_by_id(&source_id)
            .expect("Source not found");
        *self.channel_source.borrow_mut() = Some(source);
    }
}

impl WidgetImpl for Widget {
    fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
        self.on_resize(width, height);
    }

    fn snapshot(&self, snapshot: &gtk::Snapshot) {
        self.on_snapshot(snapshot);
    }
}

impl Widget {
    fn on_resize(&self, width: i32, height: i32) {
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
            self.instance().queue_draw();
            self.trigger_render();
        }
    }

    fn on_snapshot(&self, snapshot: &gtk::Snapshot) {
        let surface_size = self.surface_size.get();

        snapshot.append_color(
            &gdk::RGBA::WHITE,
            &graphene::Rect::new(0.0, 0.0, surface_size.0 as f32, surface_size.1 as f32),
        );

        if let Some(ref texture) = *self.texture.borrow() {
            snapshot.append_texture(
                texture,
                &graphene::Rect::new(
                    0.0,
                    0.0,
                    texture.width() as f32 / 2.0,
                    texture.height() as f32 / 2.0,
                ),
            );
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
                let rect = calculate_selection_rectangle(rect, surface_size);

                snapshot.append_border(
                    &gsk::RoundedRect::from_rect(
                        graphene::Rect::new(
                            rect.x as f32,
                            rect.y as f32,
                            rect.width as f32,
                            rect.height as f32,
                        ),
                        0.0,
                    ),
                    &[1.0; 4],
                    &[gdk::RGBA::new(1.0, 1.0, 1.0, 1.0); 4],
                );
                snapshot.append_color(
                    &gdk::RGBA::new(1.0, 1.0, 1.0, 0.2),
                    &graphene::Rect::new(
                        rect.x as f32,
                        rect.y as f32,
                        rect.width as f32,
                        rect.height as f32,
                    ),
                );
            }
        }
    }

    fn on_zoom_begin(&self, _controller: &gtk::GestureDrag, _x: f64, _y: f64) {
        self.zoom_controller_cancelled.set(false);
        self.move_controller.reset();
    }

    fn on_zoom_update(&self, _controller: &gtk::GestureDrag, _off_x: f64, _off_y: f64) {
        self.instance().queue_draw();
    }

    fn on_zoom_end(&self, _controller: &gtk::GestureDrag, _off_x: f64, _off_y: f64) {
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

            let _ = self.texture.borrow_mut().take();
            self.trigger_render();
        }

        self.instance().queue_draw();
    }

    fn on_zoom_cancelled(&self, _controller: &gtk::GestureDrag) {
        self.zoom_controller_cancelled.set(true);
    }

    fn on_move_begin(&self, _controller: &gtk::GestureDrag, _x: f64, _y: f64) {
        self.zoom_controller.reset();
    }

    fn on_move_update(&self, _controller: &gtk::GestureDrag, _off_x: f64, _off_y: f64) {
        self.instance().queue_draw();
        self.trigger_render();
    }

    fn on_move_end(&self, _controller: &gtk::GestureDrag, _off_x: f64, _off_y: f64) {
        if let Some((x, y)) = self.move_controller.offset() {
            let mut view = self.view.get();
            let surface_size = self.surface_size.get();
            view.x -= x / surface_size.0 as f64 * view.width;
            view.y -= y / surface_size.1 as f64 * view.height;
            self.view.set(view);

            self.instance().queue_draw();
            self.trigger_render();
        }
    }

    fn on_key_pressed(&self, keyval: gdk::Key, _keycode: u32, _state: gdk::ModifierType) {
        if keyval == gdk::Key::Escape {
            self.zoom_controller.reset();
            self.instance().queue_draw();
        }
    }

    fn on_render_done(&self, image: Image) {
        let (width, height, stride) = (
            image.width as i32,
            image.height as i32,
            image.width as usize * 4,
        );
        let texture = gdk::MemoryTexture::new(
            width,
            height,
            gdk::MemoryFormat::A8r8g8b8,
            &glib::Bytes::from_owned(image),
            stride,
        );

        *self.texture.borrow_mut() = Some(texture);
        self.instance().queue_draw();
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

impl AsRef<[u8]> for Image {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            use std::slice;

            let (ptr, len) = (self.pixels.as_ptr(), self.pixels.len());
            slice::from_raw_parts(ptr as *const u8, len * 4)
        }
    }
}

impl Pixel {
    fn new(r: u8, g: u8, b: u8) -> Self {
        Pixel { a: 255, r, g, b }
    }

    fn interpolate(self, other: Self, frac: f64) -> Self {
        Pixel::new(
            f64::clamp(
                self.r as f64 + (frac * (other.r as f64 - self.r as f64)),
                0.0,
                255.0,
            ) as u8,
            f64::clamp(
                self.g as f64 + (frac * (other.g as f64 - self.g as f64)),
                0.0,
                255.0,
            ) as u8,
            f64::clamp(
                self.b as f64 + (frac * (other.b as f64 - self.b as f64)),
                0.0,
                255.0,
            ) as u8,
        )
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
        f64::abs(rect.width / surface_size.0 as f64),
        f64::abs(rect.height / surface_size.1 as f64),
    );

    let (width, height) = if xscale > yscale {
        (
            rect.width,
            rect.height.signum()
                * f64::abs(rect.width as f64 * surface_size.1 as f64 / surface_size.0 as f64),
        )
    } else {
        (
            rect.width.signum()
                * f64::abs(rect.height as f64 * surface_size.0 as f64 / surface_size.1 as f64),
            rect.height,
        )
    };

    Rectangle {
        width,
        height,
        ..rect
    }
}

static COLORS: Lazy<[Pixel; 360]> = Lazy::new(|| {
    let mut colors = [Default::default(); 360];

    let s = 1.0;
    let v = 1.0;
    for (h, color) in colors.iter_mut().enumerate() {
        let c = v * s;
        let x = c * (1.0 - f64::abs(((h as f64) / 60.0) % 2.0 - 1.0));
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
});

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
                let log_zn = f64::ln(z.norm_sqr()) / 2.0;
                let nu = f64::ln(log_zn / f64::ln(2.0)) / f64::ln(2.0);

                let it = it as f64 + 1.0 - nu;
                let c1 = COLORS[it.floor() as usize % 360];
                let c2 = COLORS[(it.floor() + 1.0) as usize % 360];
                Pixel::interpolate(c1, c2, it.fract())
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
