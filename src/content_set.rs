use smallvec::{smallvec, SmallVec};
use tracing::{trace_span, Span};

use widget_cruncher::promise::PromiseToken;
use widget_cruncher::widget::prelude::*;
use widget_cruncher::widget::{AsWidgetPod, ClipBox, Flex, Label, Spinner, WidgetPod};
use widget_cruncher::Point;

use crate::thumbnail::Thumbnail;

pub struct ContentSetMetadata {
    pub title: String,
    pub ref_id: String,
}

pub struct ContentSet {
    pub row: usize,
    pub data: ContentSetMetadata,
    pub children_promise: PromiseToken<Vec<String>>,
    pub children: WidgetPod<Flex>,
}

// --- METHODS ---

impl ContentSet {
    pub fn new(row: usize, data: ContentSetMetadata) -> Self {
        let title_label = Label::new(data.title.clone());
        let placeholder = Spinner::new();
        Self {
            row,
            data,
            children_promise: PromiseToken::empty(),
            children: WidgetPod::new(
                Flex::column()
                    .with_child(title_label)
                    .with_child(placeholder),
            ),
        }
    }
}

fn load_content_set(url: &str) -> Result<Vec<String>, reqwest::Error> {
    let json: serde_json::Value = reqwest::blocking::get(url)?.json()?;
    let items = json["data"]["CuratedSet"]["items"].clone();
    let items_tiles = items
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|item| {
            let tileset = item["image"]["tile"].clone();
            // Just take the first suggested tile.
            let tile = tileset.as_object().unwrap().values().next()?;
            let tile_url = tile["program"]["default"]["url"].as_str()?.to_string();

            Some(tile_url)
        })
        .collect::<Vec<_>>();
    Ok(items_tiles)
}

// --- TRAIT IMPL ---

impl Widget for ContentSet {
    fn on_event(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env) {
        ctx.init();
        match event {
            Event::PromiseResult(result) => {
                if let Some(children) = result.try_get(self.children_promise) {
                    let row = self.row;
                    let title = self.data.title.clone();
                    self.children.recurse_pass(
                        "custom_pass",
                        &mut ctx.widget_state,
                        |flex, flex_state| {
                            flex.clear(flex_state);
                            flex.add_child(flex_state, Label::new(title));
                            let mut titles = Flex::row();
                            for (column, child) in children.into_iter().enumerate() {
                                titles = titles.with_child(Thumbnail::new(row, column, child));
                            }
                            flex.add_child(
                                flex_state,
                                ClipBox::new(titles).constrain_vertical(true),
                            );
                        },
                    );

                    ctx.skip_child(&mut self.children);
                    return;
                }
            }
            _ => {}
        }
        self.children.on_event(ctx, event, env)
    }

    fn on_status_change(&mut self, _ctx: &mut LifeCycleCtx, _event: &StatusChange, _env: &Env) {}

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, env: &Env) {
        let content_set_url = format!(
            "https://cd-static.bamgrid.com/dp-117731241344/sets/{}.json",
            self.data.ref_id
        );

        ctx.init();
        match event {
            LifeCycle::WidgetAdded => {
                self.children_promise =
                    ctx.compute_in_background(move |_| load_content_set(&content_set_url).unwrap());
            }
            _ => {}
        }
        self.children.lifecycle(ctx, event, env)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, env: &Env) -> Size {
        let layout = self.children.layout(ctx, bc, env);
        self.children.set_origin(ctx, env, Point::ORIGIN);
        layout
    }

    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env) {
        self.children.paint(ctx, env)
    }

    fn children(&self) -> SmallVec<[&dyn AsWidgetPod; 16]> {
        smallvec![&self.children as &dyn AsWidgetPod]
    }

    fn children_mut(&mut self) -> SmallVec<[&mut dyn AsWidgetPod; 16]> {
        smallvec![&mut self.children as &mut dyn AsWidgetPod]
    }

    fn make_trace_span(&self) -> Span {
        trace_span!("ContentSet")
    }
}
