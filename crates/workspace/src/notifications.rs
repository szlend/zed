use std::{any::TypeId, ops::DerefMut};

use collections::HashSet;
use gpui::{AnyViewHandle, Entity, MutableAppContext, View, ViewContext, ViewHandle};

use crate::Workspace;

pub fn init(cx: &mut MutableAppContext) {
    cx.set_global(NotificationTracker::new());
    simple_message_notification::init(cx);
}

pub trait Notification: View {
    fn should_dismiss_notification_on_event(&self, event: &<Self as Entity>::Event) -> bool;
}

pub trait NotificationHandle {
    fn id(&self) -> usize;
    fn to_any(&self) -> AnyViewHandle;
}

impl<T: Notification> NotificationHandle for ViewHandle<T> {
    fn id(&self) -> usize {
        self.id()
    }

    fn to_any(&self) -> AnyViewHandle {
        self.into()
    }
}

impl From<&dyn NotificationHandle> for AnyViewHandle {
    fn from(val: &dyn NotificationHandle) -> Self {
        val.to_any()
    }
}

struct NotificationTracker {
    notifications_sent: HashSet<TypeId>,
}

impl std::ops::Deref for NotificationTracker {
    type Target = HashSet<TypeId>;

    fn deref(&self) -> &Self::Target {
        &self.notifications_sent
    }
}

impl DerefMut for NotificationTracker {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.notifications_sent
    }
}

impl NotificationTracker {
    fn new() -> Self {
        Self {
            notifications_sent: HashSet::default(),
        }
    }
}

impl Workspace {
    pub fn show_notification_once<V: Notification>(
        &mut self,
        id: usize,
        cx: &mut ViewContext<Self>,
        build_notification: impl FnOnce(&mut ViewContext<Self>) -> ViewHandle<V>,
    ) {
        if !cx
            .global::<NotificationTracker>()
            .contains(&TypeId::of::<V>())
        {
            cx.update_global::<NotificationTracker, _, _>(|tracker, _| {
                tracker.insert(TypeId::of::<V>())
            });

            self.show_notification::<V>(id, cx, build_notification)
        }
    }

    pub fn show_notification<V: Notification>(
        &mut self,
        id: usize,
        cx: &mut ViewContext<Self>,
        build_notification: impl FnOnce(&mut ViewContext<Self>) -> ViewHandle<V>,
    ) {
        let type_id = TypeId::of::<V>();
        if self
            .notifications
            .iter()
            .all(|(existing_type_id, existing_id, _)| {
                (*existing_type_id, *existing_id) != (type_id, id)
            })
        {
            let notification = build_notification(cx);
            cx.subscribe(&notification, move |this, handle, event, cx| {
                if handle.read(cx).should_dismiss_notification_on_event(event) {
                    this.dismiss_notification(type_id, id, cx);
                }
            })
            .detach();
            self.notifications
                .push((type_id, id, Box::new(notification)));
            cx.notify();
        }
    }

    fn dismiss_notification(&mut self, type_id: TypeId, id: usize, cx: &mut ViewContext<Self>) {
        self.notifications
            .retain(|(existing_type_id, existing_id, _)| {
                if (*existing_type_id, *existing_id) == (type_id, id) {
                    cx.notify();
                    false
                } else {
                    true
                }
            });
    }
}

pub mod simple_message_notification {
    use std::process::Command;

    use gpui::{
        actions,
        elements::{Flex, MouseEventHandler, Padding, ParentElement, Svg, Text},
        impl_actions, Action, CursorStyle, Element, Entity, MouseButton, MutableAppContext, View,
        ViewContext,
    };
    use menu::Cancel;
    use serde::Deserialize;
    use settings::Settings;

    use crate::Workspace;

    use super::Notification;

    actions!(message_notifications, [CancelMessageNotification]);

    #[derive(Clone, Default, Deserialize, PartialEq)]
    pub struct OsOpen(pub String);

    impl_actions!(message_notifications, [OsOpen]);

    pub fn init(cx: &mut MutableAppContext) {
        cx.add_action(MessageNotification::dismiss);
        cx.add_action(
            |_workspace: &mut Workspace, open_action: &OsOpen, _cx: &mut ViewContext<Workspace>| {
                #[cfg(target_os = "macos")]
                {
                    let mut command = Command::new("open");
                    command.arg(open_action.0.clone());

                    command.spawn().ok();
                }
            },
        )
    }

    pub struct MessageNotification {
        message: String,
        click_action: Box<dyn Action>,
        click_message: String,
    }

    pub enum MessageNotificationEvent {
        Dismiss,
    }

    impl Entity for MessageNotification {
        type Event = MessageNotificationEvent;
    }

    impl MessageNotification {
        pub fn new<S1: AsRef<str>, A: Action, S2: AsRef<str>>(
            message: S1,
            click_action: A,
            click_message: S2,
        ) -> Self {
            Self {
                message: message.as_ref().to_string(),
                click_action: Box::new(click_action) as Box<dyn Action>,
                click_message: click_message.as_ref().to_string(),
            }
        }

        pub fn dismiss(&mut self, _: &CancelMessageNotification, cx: &mut ViewContext<Self>) {
            cx.emit(MessageNotificationEvent::Dismiss);
        }
    }

    impl View for MessageNotification {
        fn ui_name() -> &'static str {
            "MessageNotification"
        }

        fn render(&mut self, cx: &mut gpui::RenderContext<'_, Self>) -> gpui::ElementBox {
            let theme = cx.global::<Settings>().theme.clone();
            let theme = &theme.update_notification;

            enum MessageNotificationTag {}

            let click_action = self.click_action.boxed_clone();
            let click_message = self.click_message.clone();
            let message = self.message.clone();

            MouseEventHandler::<MessageNotificationTag>::new(0, cx, |state, cx| {
                Flex::column()
                    .with_child(
                        Flex::row()
                            .with_child(
                                Text::new(message, theme.message.text.clone())
                                    .contained()
                                    .with_style(theme.message.container)
                                    .aligned()
                                    .top()
                                    .left()
                                    .flex(1., true)
                                    .boxed(),
                            )
                            .with_child(
                                MouseEventHandler::<Cancel>::new(0, cx, |state, _| {
                                    let style = theme.dismiss_button.style_for(state, false);
                                    Svg::new("icons/x_mark_8.svg")
                                        .with_color(style.color)
                                        .constrained()
                                        .with_width(style.icon_width)
                                        .aligned()
                                        .contained()
                                        .with_style(style.container)
                                        .constrained()
                                        .with_width(style.button_width)
                                        .with_height(style.button_width)
                                        .boxed()
                                })
                                .with_padding(Padding::uniform(5.))
                                .on_click(MouseButton::Left, move |_, cx| {
                                    cx.dispatch_action(CancelMessageNotification)
                                })
                                .aligned()
                                .constrained()
                                .with_height(
                                    cx.font_cache().line_height(theme.message.text.font_size),
                                )
                                .aligned()
                                .top()
                                .flex_float()
                                .boxed(),
                            )
                            .boxed(),
                    )
                    .with_child({
                        let style = theme.action_message.style_for(state, false);

                        Text::new(click_message, style.text.clone())
                            .contained()
                            .with_style(style.container)
                            .boxed()
                    })
                    .contained()
                    .boxed()
            })
            .with_cursor_style(CursorStyle::PointingHand)
            .on_click(MouseButton::Left, move |_, cx| {
                cx.dispatch_any_action(click_action.boxed_clone())
            })
            .boxed()
        }
    }

    impl Notification for MessageNotification {
        fn should_dismiss_notification_on_event(&self, event: &<Self as Entity>::Event) -> bool {
            match event {
                MessageNotificationEvent::Dismiss => true,
            }
        }
    }
}