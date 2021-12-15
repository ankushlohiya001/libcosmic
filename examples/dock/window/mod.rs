mod imp;
// use crate::ApplicationObject;
use crate::dock_item::DockItem;
use crate::X11_CONN;
use gdk4::Rectangle;
use gdk4::Surface;
use gdk4_x11::X11Surface;
use gio::DesktopAppInfo;
use gtk4 as gtk;
use gtk4::EventControllerMotion;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt;

// use crate::application_row::ApplicationRow;
use glib::Object;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use gtk::{Application, SignalListItemFactory};
use libcosmic::x;
use x11rb::protocol::xproto;

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl Window {
    pub fn new(app: &Application) -> Self {
        let self_: Self = Object::new(&[("application", app)]).expect("Failed to create `Window`.");
        self_
    }

    pub fn model(&self) -> &gio::ListStore {
        // Get state
        let imp = imp::Window::from_instance(self);
        imp.model.get().expect("Could not get model")
    }

    fn setup_model(&self) {
        // Get state and set model

        let imp = imp::Window::from_instance(self);
        let model = gio::ListStore::new(DesktopAppInfo::static_type());

        let selection_model = gtk::SingleSelection::builder()
            .autoselect(false)
            .can_unselect(true)
            .selected(gtk4::INVALID_LIST_POSITION)
            .model(&model)
            .build();
        xdg::BaseDirectories::new()
            .expect("could not access XDG Base directory")
            .get_data_dirs()
            .iter_mut()
            .for_each(|xdg_data_path| {
                let defaults = ["Firefox Web Browser", "Files", "Terminal", "Pop!_Shop"];
                xdg_data_path.push("applications");
                dbg!(&xdg_data_path);
                if let Ok(dir_iter) = std::fs::read_dir(xdg_data_path) {
                    dir_iter.for_each(|dir_entry| {
                        if let Ok(dir_entry) = dir_entry {
                            if let Some(path) = dir_entry.path().file_name() {
                                if let Some(path) = path.to_str() {
                                    if let Some(app_info) = gio::DesktopAppInfo::new(path) {
                                        if app_info.should_show()
                                            && defaults.contains(&app_info.name().as_str())
                                        {
                                            dbg!(app_info.name());
                                            model.append(&app_info)
                                        } else {
                                            // println!("Ignoring {}", path);
                                        }
                                    } else {
                                        // println!("error loading {}", path);
                                    }
                                }
                            }
                        }
                    })
                }
            });

        imp.model.set(model).expect("Could not set model");
        // Wrap model with selection and pass it to the list view
        imp.list_view.set_model(Some(&selection_model));
    }

    fn setup_callbacks(&self) {
        // Get state
        let imp = imp::Window::from_instance(self);
        let window = self.clone().upcast::<gtk::Window>();
        let list_view = &imp.list_view;

        let app_selection_model = list_view
            .model()
            .expect("List view missing selection model")
            .downcast::<gtk::SingleSelection>()
            .expect("could not downcast listview model to single selection model");

        app_selection_model.connect_selected_notify(glib::clone!(@weak window => move |model| {
            let position = model.selected();
            println!("selected app {}", position);
            // Launch the application when an item of the list is activated
            if let Some(item) = model.item(position) {
                let app_info = item.downcast::<gio::DesktopAppInfo>().unwrap();
                let context = window.display().app_launch_context();
                if let Err(err) = app_info.launch(&[], Some(&context)) {
                    gtk::MessageDialog::builder()
                        .text(&format!("Failed to start {}", app_info.name()))
                        .secondary_text(&err.to_string())
                        .message_type(gtk::MessageType::Error)
                        .modal(true)
                        .transient_for(&window)
                        .build()
                        .show();
                }
            }
        }));

        let enter_event_controller = &imp.enter_event_controller.get().unwrap();
        let leave_event_controller = &imp.leave_event_controller.get().unwrap();
        let revealer = &imp.revealer.get();
        window.connect_show(
            glib::clone!(@weak revealer, @weak leave_event_controller => move |_| {
                dbg!(!leave_event_controller.contains_pointer());
                if !leave_event_controller.contains_pointer() {
                    revealer.set_reveal_child(false);
                }
            }),
        );
        window.connect_realize(move |window| {
            if let Some((display, surface)) = x::get_window_x11(window) {
                unsafe {
                    x::change_property(
                        &display,
                        &surface,
                        "_NET_WM_WINDOW_TYPE",
                        x::PropMode::Replace,
                        &[x::Atom::new(&display, "_NET_WM_WINDOW_TYPE_DOCK").unwrap()],
                    );
                }
                let s = window.surface().expect("Failed to get Surface for Window");
                let surface_resize_handler = glib::clone!(@weak window => move |s: &Surface| {
                    if let Some((display, _surface)) = x::get_window_x11(&window) {
                        let width = s.width() * s.scale_factor();
                        let height = s.height() * s.scale_factor();
                        let monitor = display
                            .primary_monitor()
                            .expect("Failed to get Monitor");
                        let Rectangle {
                            x: monitor_x,
                            y: monitor_y,
                            width: monitor_width,
                            height: monitor_height,
                        } = monitor.geometry();
                        dbg!(monitor_width);
                        dbg!(monitor_height);
                        dbg!(width);
                        dbg!(height);
                        let w_conf = xproto::ConfigureWindowAux::default()
                            .x(monitor_x + monitor_width / 2 - width / 2)
                            .y(monitor_y + monitor_height - height);
                        let conn = X11_CONN.get().expect("Failed to get X11_CONN");

                        let x11surface = gdk4_x11::X11Surface::xid(
                            &s.clone().downcast::<X11Surface>()
                                .expect("Failed to downcast Surface to X11Surface"),
                        );
                        conn.configure_window(
                            x11surface.try_into().expect("Failed to convert XID"),
                            &w_conf,
                        )
                        .expect("failed to configure window...");
                        conn.flush().expect("failed to flush");

                   } else {
                        println!("failed to get X11 window");
                    }
                });
                s.connect_height_notify(surface_resize_handler.clone());
                s.connect_width_notify(surface_resize_handler.clone());
                s.connect_scale_factor_notify(surface_resize_handler);
            } else {
                println!("failed to get X11 window");
            }
        });
        enter_event_controller.connect_enter(glib::clone!(@weak revealer => move |_evc, _x, _y| {
            dbg!("hello, mouse entered me :)");
            revealer.set_reveal_child(true);
        }));
        leave_event_controller.connect_leave(glib::clone!(@weak revealer => move |_evc| {
            dbg!("hello, mouse left me :)");
            revealer.set_reveal_child(false);
        }));
    }

    fn setup_event_controller(&self) {
        let imp = imp::Window::from_instance(self);
        let enter_handle = &imp.cursor_enter_handle.get();
        let enter_ev = EventControllerMotion::builder()
            .propagation_limit(gtk4::PropagationLimit::None)
            .propagation_phase(gtk4::PropagationPhase::Capture)
            .build();
        enter_handle.add_controller(&enter_ev);
        let leave_handle = &imp.cursor_leave_handle.get();
        let leave_ev = EventControllerMotion::builder()
            .propagation_limit(gtk4::PropagationLimit::None)
            .propagation_phase(gtk4::PropagationPhase::Capture)
            .build();
        enter_handle.add_controller(&enter_ev);
        leave_handle.add_controller(&leave_ev);
        imp.enter_event_controller
            .set(enter_ev)
            .expect("Could not set event controller");
        imp.leave_event_controller
            .set(leave_ev)
            .expect("Could not set event controller");
    }

    fn setup_factory(&self) {
        let factory = SignalListItemFactory::new();
        factory.connect_setup(move |_, list_item| {
            let dock_item = DockItem::new();
            list_item.set_child(Some(&dock_item));
        });
        factory.connect_bind(move |_, list_item| {
            let application_object = list_item
                .item()
                .expect("The item has to exist.")
                .downcast::<DesktopAppInfo>()
                .expect("The item has to be a `DesktopAppInfo`");
            let dock_item = list_item
                .child()
                .expect("The list item child needs to exist.")
                .downcast::<DockItem>()
                .expect("The list item type needs to be `DockItem`");

            dock_item.set_app_info(&application_object);
        });
        // Set the factory of the list view
        let imp = imp::Window::from_instance(self);
        imp.list_view.set_factory(Some(&factory));
    }
}