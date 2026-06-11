// Lucide-style SVG icon components for Dioxus 0.8.
// Path data sourced from lucide.dev (ISC licence).
// Each component accepts `size: usize` (default 24) and `class: Option<String>`.

use dioxus::prelude::*;

macro_rules! icon {
    ($name:ident, $($body:tt)*) => {
        #[derive(Clone, PartialEq, Props)]
        pub struct $name {
            #[props(default = 24)]
            pub size: usize,
            pub class: Option<String>,
        }
        #[component]
        pub fn $name(props: $name) -> Element {
            rsx! {
                svg {
                    "xmlns": "http://www.w3.org/2000/svg",
                    class: if let Some(c) = props.class { "{c}" },
                    width: "{props.size}",
                    height: "{props.size}",
                    "viewBox": "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    "stroke-width": "2",
                    "stroke-linecap": "round",
                    "stroke-linejoin": "round",
                    $($body)*
                }
            }
        }
    };
}

icon!(IcoFilePlus,
    path { d: "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" }
    path { d: "M14 2v5a1 1 0 0 0 1 1h5" }
    path { d: "M9 15h6" }
    path { d: "M12 18v-6" }
);

icon!(IcoFolderPlus,
    path { d: "M12 10v6" }
    path { d: "M9 13h6" }
    path { d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" }
);

icon!(IcoCalendar,
    path { d: "M8 2v4" }
    path { d: "M16 2v4" }
    rect { "width": "18", "height": "18", "x": "3", "y": "4", "rx": "2" }
    path { d: "M3 10h18" }
);

icon!(IcoSettings,
    path { d: "M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915" }
    circle { "cx": "12", "cy": "12", "r": "3" }
);

icon!(IcoX,
    path { d: "M18 6 6 18" }
    path { d: "m6 6 12 12" }
);

icon!(IcoFolderTree,
    path { d: "M20 10a1 1 0 0 0 1-1V6a1 1 0 0 0-1-1h-2.5a1 1 0 0 1-.8-.4l-.9-1.2A1 1 0 0 0 15 3h-2a1 1 0 0 0-1 1v5a1 1 0 0 0 1 1Z" }
    path { d: "M20 21a1 1 0 0 0 1-1v-3a1 1 0 0 0-1-1h-2.9a1 1 0 0 1-.88-.55l-.42-.85a1 1 0 0 0-.92-.6H13a1 1 0 0 0-1 1v5a1 1 0 0 0 1 1Z" }
    path { d: "M3 5a2 2 0 0 0 2 2h3" }
    path { d: "M3 3v13a2 2 0 0 0 2 2h3" }
);

icon!(IcoSearch,
    circle { "cx": "11", "cy": "11", "r": "8" }
    path { d: "m21 21-4.34-4.34" }
);

icon!(IcoLink2,
    path { d: "M9 17H7A5 5 0 0 1 7 7h2" }
    path { d: "M15 7h2a5 5 0 1 1 0 10h-2" }
    line { "x1": "8", "x2": "16", "y1": "12", "y2": "12" }
);

icon!(IcoNetwork,
    rect { "x": "16", "y": "16", "width": "6", "height": "6", "rx": "1" }
    rect { "x": "2", "y": "16", "width": "6", "height": "6", "rx": "1" }
    rect { "x": "9", "y": "2", "width": "6", "height": "6", "rx": "1" }
    path { d: "M5 16v-3a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v3" }
    path { d: "M12 12V8" }
);

icon!(IcoBookmark,
    path { d: "M17 3a2 2 0 0 1 2 2v15a1 1 0 0 1-1.496.868l-4.512-2.578a2 2 0 0 0-1.984 0l-4.512 2.578A1 1 0 0 1 5 20V5a2 2 0 0 1 2-2z" }
);

icon!(IcoBookmarkCheck,
    path { d: "M17 3a2 2 0 0 1 2 2v15a1 1 0 0 1-1.496.868l-4.512-2.578a2 2 0 0 0-1.984 0l-4.512 2.578A1 1 0 0 1 5 20V5a2 2 0 0 1 2-2z" }
    path { d: "m9 10 2 2 4-4" }
);

icon!(IcoFolderKanban,
    path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.93a2 2 0 0 1-1.66-.9l-.82-1.2A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
    path { d: "M8 10v4" }
    path { d: "M12 10v2" }
    path { d: "M16 10v6" }
);

icon!(IcoLayoutList,
    rect { "x": "3", "y": "3", "width": "7", "height": "7", "rx": "1" }
    rect { "x": "3", "y": "14", "width": "7", "height": "7", "rx": "1" }
    path { d: "M14 4h7" }
    path { d: "M14 9h7" }
    path { d: "M14 15h7" }
    path { d: "M14 20h7" }
);

icon!(IcoListChecks,
    path { d: "m3 17 2 2 4-4" }
    path { d: "m3 7 2 2 4-4" }
    path { d: "M13 6h8" }
    path { d: "M13 12h8" }
    path { d: "M13 18h8" }
);

icon!(IcoChevronLeft,
    path { d: "m15 18-6-6 6-6" }
);

icon!(IcoChevronRight,
    path { d: "m9 18 6-6-6-6" }
);

icon!(IcoChevronDown,
    path { d: "m6 9 6 6 6-6" }
);

icon!(IcoDownload,
    path { d: "M12 15V3" }
    path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
    path { d: "m7 10 5 5 5-5" }
);

icon!(IcoFileText,
    path { d: "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" }
    path { d: "M14 2v5a1 1 0 0 0 1 1h5" }
    path { d: "M10 9H8" }
    path { d: "M16 13H8" }
    path { d: "M16 17H8" }
);

icon!(IcoFolderClosed,
    path { d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" }
    path { d: "M2 10h20" }
);

icon!(IcoFolderOpen,
    path { d: "m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" }
);

icon!(IcoTrash2,
    path { d: "M10 11v6" }
    path { d: "M14 11v6" }
    path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" }
    path { d: "M3 6h18" }
    path { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
);
