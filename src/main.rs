#![allow(dead_code)]

mod ui;

use gpui_icons::LucideIcon;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled, Window, WindowDecorations, WindowOptions, div, point,
    prelude::FluentBuilder as _, px,
};
use ui::{
    app::{AppConfig, run},
    app_theme,
    input::{Input, InputEvent, InputState},
    sidebar::{
        Sidebar, SidebarItem, SidebarLayout, SidebarState, sidebar_is_mobile, sidebar_menu,
        sidebar_separator,
    },
    theme::UiTheme,
};

/// 自绘标题栏高度；macOS 红绿灯在其中的垂直位置据此居中。
const TITLEBAR_HEIGHT: f32 = 40.;

fn main() {
    // Opens the window with the Lucide icons, the HarmonyOS Sans fonts and the
    // theme pair from assets/theme.css; `gpuicn theme set` changes the base
    // color, radius and appearance.
    run(
        AppConfig::new("Firefly Music (Player)")
            .themes(app_theme::light(), app_theme::dark())
            .appearance(app_theme::APPEARANCE)
            .window_options(WindowOptions {
                // macOS / Windows：隐藏系统标题栏，改用自绘标题栏。
                // Linux（X11/Wayland）：用客户端装饰隐藏服务端标题栏。
                titlebar: Some(gpui_kit::TitlebarOptions {
                    title: None, // 由 AppConfig::install 填入窗口标题
                    appears_transparent: true,
                    // macOS 系统红绿灯左上角坐标，垂直居中于自绘标题栏。
                    traffic_light_position: Some(point(px(16.), px((TITLEBAR_HEIGHT - 14.) / 2.))),
                }),
                window_decorations: Some(WindowDecorations::Client),
                // macOS：自绘标题栏时由应用自己处理拖动（start_window_move）。
                app_owns_titlebar_drag: true,
                ..Default::default()
            }),
        |window, cx| Player::new(window, cx),
    );
}

/// 侧边栏的导航目的地。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Destination {
    Home,
    Library,
    Artists,
    Albums,
    Liked,
    Favorites,
}

impl Destination {
    /// 全部目的地，按侧边栏顺序排列。
    const ALL: [Self; 6] = [
        Self::Home,
        Self::Library,
        Self::Artists,
        Self::Albums,
        Self::Liked,
        Self::Favorites,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Home => "主页",
            Self::Library => "音乐库",
            Self::Artists => "艺术家",
            Self::Albums => "专辑",
            Self::Liked => "我喜欢的",
            Self::Favorites => "我收藏的",
        }
    }

    fn icon(self) -> LucideIcon {
        match self {
            Self::Home => LucideIcon::House,
            Self::Library => LucideIcon::Library,
            Self::Artists => LucideIcon::UserRound,
            Self::Albums => LucideIcon::Disc3,
            Self::Liked => LucideIcon::Heart,
            Self::Favorites => LucideIcon::Bookmark,
        }
    }

    /// 稳定的英文键，用于元素 id 与英文关键词匹配。
    fn key(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Library => "library",
            Self::Artists => "artists",
            Self::Albums => "albums",
            Self::Liked => "liked",
            Self::Favorites => "favorites",
        }
    }
}

struct Player {
    /// 当前选中的导航项。
    active: Destination,
    /// 侧边栏外壳状态（桌面展开 / 移动端抽屉）。
    sidebar: SidebarState,
    /// spotlight 搜索的输入状态。
    search: Entity<InputState>,
    /// spotlight 是否打开。
    search_open: bool,
}

impl Player {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder("在这里搜索歌曲、歌手、专辑信息")
        });
        // 输入变化时重绘结果；回车选中第一条结果并关闭。
        cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) && this.search_open {
                if let Some(first) = this.filtered(&*cx).into_iter().next() {
                    this.active = first;
                }
                this.search_open = false;
            }
            cx.notify();
        })
        .detach();

        Self {
            active: Destination::Home,
            sidebar: SidebarState::default(),
            search,
            search_open: false,
        }
    }

    /// 打开 spotlight：清空上一次输入并把焦点交给搜索框。
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_open = true;
        self.search
            .update(cx, |state, cx| state.set_value("", window, cx));
        let focus = self.search.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// 按当前输入过滤目的地（匹配中文名或英文键）。
    fn filtered(&self, cx: &App) -> Vec<Destination> {
        let query = self.search.read(cx).value().to_lowercase();
        Destination::ALL
            .into_iter()
            .filter(|d| {
                query.is_empty()
                    || d.label().to_lowercase().contains(&query)
                    || d.key().contains(&query)
            })
            .collect()
    }

    /// 构造一行导航项。id 保持稳定，折叠成图标栏时焦点也能保留。
    fn nav_item(
        &self,
        id: &'static str,
        destination: Destination,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = UiTheme::read(cx).clone();
        let selected = self.active == destination;
        let icon_color = if selected {
            theme.colors.sidebar_accent_foreground
        } else {
            theme.colors.sidebar_foreground
        };
        SidebarItem::new(id, destination.label())
            .selected(selected)
            .h(px(42.))
            .icon(
                gpui_icons::lucide(destination.icon())
                    .size(theme.spacing.unit * 5_f32)
                    .text_color(icon_color),
            )
            .on_activate(cx.listener(move |this, _, _, cx| {
                this.active = destination;
                cx.notify();
            }))
            .into_any_element()
    }

    /// 一个窗口控制按钮（最小化 / 最大化还原 / 关闭）。
    fn window_button(
        &self,
        id: &'static str,
        icon: LucideIcon,
        danger: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = UiTheme::read(cx).clone();
        let s = theme.spacing.unit;
        let fg = theme.colors.sidebar_foreground;
        let hover_bg = theme.colors.sidebar_accent;
        let hover_fg = theme.colors.sidebar_accent_foreground;
        div()
            .id(id)
            .h_full()
            .w(s * 11_f32)
            .flex()
            .items_center()
            .justify_center()
            .text_color(fg)
            // 避免按下按钮时冒泡触发标题栏拖动窗口。
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .hover(move |style| {
                if danger {
                    style.bg(gpui_kit::rgb(0xe81123)).text_color(gpui_kit::rgb(0xffffff))
                } else {
                    style.bg(hover_bg).text_color(hover_fg)
                }
            })
            .on_click(move |_, window, app| action(window, app))
            .child(gpui_icons::lucide(icon).size(s * 3.5_f32))
            .into_any_element()
    }

    /// 自绘标题栏：macOS 用系统红绿灯，Windows/Linux 用 lucide 按钮。
    fn titlebar(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = UiTheme::read(cx).clone();
        let s = theme.spacing.unit;
        let is_mac = cx.platform_info().is_mac();

        // 左侧品牌区：macOS 为红绿灯预留空间；Windows/Linux 直接贴最左端。
        let leading = div()
            .flex()
            .items_center()
            .gap(s * 2_f32)
            .when(is_mac, |el| el.pl(px(78.)))
            .child(
                gpui_icons::lucide(LucideIcon::Music2)
                    .size(s * 4_f32)
                    .text_color(theme.colors.sidebar_foreground),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s * 1.5_f32)
                    .child(
                        div()
                            .text_size(theme.text(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.colors.sidebar_foreground)
                            .child("Firefly Music"),
                    )
                    .child(
                        // 黄色标签
                        div()
                            .flex()
                            .items_center()
                            .px(s * 1.5_f32)
                            .py(px(2.))
                            .rounded_full()
                            .bg(gpui_kit::rgb(0xfacc15))
                            .text_color(gpui_kit::rgb(0x422006))
                            .text_size(theme.text(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Player"),
                    ),
            );

        // 中央搜索触发框：外观像输入框，点击打开 spotlight。
        let search_hover = theme.colors.sidebar_accent;
        let search_trigger = div()
            .id("titlebar.search")
            .h(px(28.))
            .w(px(320.))
            .min_w_0()
            .flex()
            .items_center()
            .gap(s * 2_f32)
            .px(s * 3_f32)
            .rounded_full()
            .bg(search_hover)
            .border_1()
            .border_color(theme.colors.sidebar_border)
            .text_color(theme.colors.muted_foreground)
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .hover(move |style| style.bg(search_hover.opacity(0.7)))
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_search(window, cx);
            }))
            .child(
                gpui_icons::lucide(LucideIcon::Search)
                    .size(s * 3.5_f32)
                    .text_color(theme.colors.muted_foreground),
            )
            .child(
                div()
                    .text_size(theme.text(12.5))
                    .child("在这里搜索歌曲 歌手 专辑信息"),
            );

        // 右侧窗口控制按钮（仅 Windows / Linux）。
        let controls: Vec<AnyElement> = if is_mac {
            Vec::new()
        } else {
            let maximize_icon = if window.is_maximized() {
                LucideIcon::Minimize2
            } else {
                LucideIcon::Maximize2
            };
            vec![
                self.window_button(
                    "titlebar.minimize",
                    LucideIcon::Minus,
                    false,
                    cx,
                    |window, _| window.minimize_window(),
                ),
                self.window_button(
                    "titlebar.zoom",
                    maximize_icon,
                    false,
                    cx,
                    |window, _| window.zoom_window(),
                ),
                self.window_button("titlebar.close", LucideIcon::X, true, cx, |_, app| {
                    app.quit()
                }),
            ]
        };

        div()
            .id("app.titlebar")
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(TITLEBAR_HEIGHT))
            .bg(theme.colors.sidebar)
            .border_b_1()
            .border_color(theme.colors.sidebar_border)
            .on_mouse_down(MouseButton::Left, |event, window, _| {
                // 双击切换最大化/还原，单击拖动窗口。
                if event.click_count == 2 {
                    window.titlebar_double_click();
                } else {
                    window.start_window_move();
                }
            })
            // 三段式：左右各占一份剩余空间，中间固定宽度，从而精确居中。
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .child(leading),
            )
            .child(search_trigger)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_end()
                    .children(controls),
            )
            .into_any_element()
    }

    /// 窗口中央的 spotlight 搜索弹层。
    fn spotlight(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = UiTheme::read(cx).clone();
        let s = theme.spacing.unit;
        let results = self.filtered(&*cx);

        let rows: Vec<AnyElement> = if results.is_empty() {
            vec![
                div()
                    .px(s * 3_f32)
                    .py(s * 4_f32)
                    .text_size(theme.text(13.))
                    .text_color(theme.colors.muted_foreground)
                    .child("没有找到匹配的结果")
                    .into_any_element(),
            ]
        } else {
            results
                .into_iter()
                .map(|destination| {
                    let accent = theme.colors.accent;
                    let accent_fg = theme.colors.accent_foreground;
                    div()
                        .id(format!("spotlight.{}", destination.key()))
                        .flex()
                        .items_center()
                        .gap(s * 2.5_f32)
                        .px(s * 3_f32)
                        .py(s * 2.25_f32)
                        .rounded(theme.radius.md)
                        .text_color(theme.colors.popover_foreground)
                        .cursor_pointer()
                        .hover(move |style| style.bg(accent).text_color(accent_fg))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.active = destination;
                            this.search_open = false;
                            cx.notify();
                        }))
                        .child(
                            gpui_icons::lucide(destination.icon())
                                .size(s * 4_f32)
                                .text_color(theme.colors.muted_foreground),
                        )
                        .child(
                            div()
                                .text_size(theme.text(13.))
                                .child(destination.label()),
                        )
                        .into_any_element()
                })
                .collect()
        };

        let panel = div()
            .w(px(560.))
            .flex()
            .flex_col()
            .rounded(theme.radius.xl)
            .bg(theme.colors.popover)
            .text_color(theme.colors.popover_foreground)
            .border_1()
            .border_color(theme.colors.border)
            .shadow(theme.shadows.lg.to_vec())
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(s * 2.5_f32)
                    .px(s * 4_f32)
                    .py(s * 3_f32)
                    .child(
                        gpui_icons::lucide(LucideIcon::Search)
                            .size(s * 4.5_f32)
                            .text_color(theme.colors.muted_foreground),
                    )
                    .child(div().flex_1().min_w_0().child(Input::new(&self.search))),
            )
            .child(div().h(px(1.)).bg(theme.colors.border))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .p(s * 2_f32)
                    .children(rows),
            );

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .p(s * 6_f32)
            .bg(theme.colors.overlay)
            .occlude()
            // 点击遮罩关闭；点击面板时阻止冒泡。
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.search_open = false;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.search_open = false;
                    cx.notify();
                }
            }))
            .child(
                panel.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            )
            .into_any_element()
    }
}

impl Render for Player {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = UiTheme::read(cx).clone();
        let mobile = sidebar_is_mobile(window, cx);

        // 侧边栏：顶部品牌 + 两组导航，中间用分割线隔开。
        let sidebar = Sidebar::new("player.sidebar", "主导航")
            .header(
                div()
                    .flex()
                    .items_center()
                    .gap(theme.spacing.unit * 2_f32),
            )
            .child(sidebar_menu().gap(theme.spacing.unit).children([
                self.nav_item("nav.home", Destination::Home, cx),
                self.nav_item("nav.library", Destination::Library, cx),
                self.nav_item("nav.artists", Destination::Artists, cx),
                self.nav_item("nav.albums", Destination::Albums, cx),
            ]))
            .child(sidebar_separator(cx))
            .child(sidebar_menu().gap(theme.spacing.unit).children([
                self.nav_item("nav.liked", Destination::Liked, cx),
                self.nav_item("nav.favorites", Destination::Favorites, cx),
            ]));

        // 内容区占位：显示当前选中的导航目的地。
        let content = div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(theme.spacing.unit * 2_f32)
            .bg(theme.colors.background)
            .text_color(theme.colors.foreground)
            .font_family(theme.fonts.body.clone())
            .child(
                div()
                    .text_size(px(30.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.active.label()),
            )
            .child(
                div()
                    .text_size(theme.text(14.))
                    .text_color(theme.colors.muted_foreground)
                    .child("从左侧导航选择页面"),
            );

        // 标题栏容器套在侧边栏（及内容区）之上，横跨窗口整个宽度。
        let mut root = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(theme.colors.background)
            .text_color(theme.colors.foreground)
            .font_family(theme.fonts.body.clone())
            .child(self.titlebar(window, cx))
            .child(
                SidebarLayout::new("player.layout", self.sidebar)
                    .flex_1()
                    .mobile(mobile)
                    .sidebar(sidebar)
                    .content(content)
                    .on_change(cx.listener(|this, state: &SidebarState, _, cx| {
                        this.sidebar = *state;
                        cx.notify();
                    })),
            );

        // spotlight 覆盖在窗口中央。
        if self.search_open {
            root = root.child(self.spotlight(cx));
        }

        root
    }
}
