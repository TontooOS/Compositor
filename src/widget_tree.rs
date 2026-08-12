//! Widget tree parser — deserializes client widget trees and converts
//! them to DrawCommands for server-side rendering.

use crate::widget_renderer::{Color, DrawCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WidgetType {
    Text = 1,
    Button = 2,
    Image = 3,
    Toggle = 4,
    TextField = 5,
    Divider = 6,
    Spacer = 7,
    Card = 8,
    VStack = 10,
    HStack = 11,
    ZStack = 12,
    Padding = 13,
    Frame = 14,
}

#[derive(Debug, Clone)]
pub struct FlatWidget {
    pub widget_type: WidgetType,
    pub node_id: usize,
    pub bounds: [f32; 4],
    pub properties: WidgetProperties,
    pub children: Vec<usize>,
    pub interactive: bool,
}

#[derive(Debug, Clone)]
pub enum WidgetProperties {
    Text { content: String, font_size: f32, color: Color, max_width: Option<f32> },
    Button { label: String, background: Color, text_color: Color, corner_radius: f32 },
    Image { path: String },
    Toggle { is_on: bool, label: String },
    TextField { placeholder: String, value: String },
    Divider { color: Color, thickness: f32 },
    Spacer { min_length: f32 },
    Card { background: Color, corner_radius: f32, milkiness: f32 },
    VStack { spacing: f32, alignment: u8 },
    HStack { spacing: f32, alignment: u8 },
    ZStack,
    Padding { top: f32, right: f32, bottom: f32, left: f32 },
    Frame { width: Option<f32>, height: Option<f32> },
}

struct BinaryReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BinaryReader<'a> {
    fn new(data: &'a [u8]) -> Self { Self { data, pos: 0 } }

    fn read_u8(&mut self) -> Option<u8> {
        if self.pos >= self.data.len() { return None; }
        let v = self.data[self.pos]; self.pos += 1; Some(v)
    }

    fn read_u32(&mut self) -> Option<u32> {
        if self.pos + 4 > self.data.len() { return None; }
        let v = u32::from_le_bytes([self.data[self.pos], self.data[self.pos+1], self.data[self.pos+2], self.data[self.pos+3]]);
        self.pos += 4; Some(v)
    }

    fn read_f32(&mut self) -> Option<f32> {
        self.read_u32().map(f32::from_bits)
    }

    fn read_string(&mut self) -> Option<String> {
        let len = self.read_u32()? as usize;
        if self.pos + len > self.data.len() { return None; }
        let bytes = &self.data[self.pos..self.pos+len];
        self.pos += len;
        String::from_utf8(bytes.to_vec()).ok()
    }

    fn read_color(&mut self) -> Option<Color> {
        let r = self.read_f32()?;
        let g = self.read_f32()?;
        let b = self.read_f32()?;
        let a = self.read_f32()?;
        Some(Color::new(r, g, b, a))
    }

    fn read_optional_f32(&mut self) -> Option<Option<f32>> {
        let tag = self.read_u8()?;
        if tag == 1 { self.read_f32().map(Some) } else { Some(None) }
    }
}

pub fn parse_widget_tree(data: &[u8]) -> Option<Vec<FlatWidget>> {
    let mut r = BinaryReader::new(data);
    let node_count = r.read_u32()? as usize;
    let mut widgets = Vec::with_capacity(node_count);

    for node_id in 0..node_count {
        let type_tag = r.read_u8()?;
        let widget_type = match type_tag {
            1 => WidgetType::Text, 2 => WidgetType::Button, 3 => WidgetType::Image,
            4 => WidgetType::Toggle, 5 => WidgetType::TextField, 6 => WidgetType::Divider,
            7 => WidgetType::Spacer, 8 => WidgetType::Card, 10 => WidgetType::VStack,
            11 => WidgetType::HStack, 12 => WidgetType::ZStack, 13 => WidgetType::Padding,
            14 => WidgetType::Frame, _ => return None,
        };
        let properties = decode_properties(&mut r, widget_type)?;
        let bx = r.read_f32()?;
        let by = r.read_f32()?;
        let bw = r.read_f32()?;
        let bh = r.read_f32()?;
        let child_count = r.read_u32()? as usize;
        let mut children = Vec::with_capacity(child_count);
        for _ in 0..child_count {
            children.push(r.read_u32()? as usize);
        }
        let interactive = matches!(widget_type,
            WidgetType::Button | WidgetType::Toggle | WidgetType::TextField);
        widgets.push(FlatWidget { widget_type, node_id, bounds: [bx, by, bw, bh], properties, children, interactive });
    }
    Some(widgets)
}

fn decode_properties(r: &mut BinaryReader, widget_type: WidgetType) -> Option<WidgetProperties> {
    match widget_type {
        WidgetType::Text => {
            let content = r.read_string()?;
            let font_size = r.read_f32()?;
            let color = r.read_color()?;
            let max_width = r.read_optional_f32()?;
            Some(WidgetProperties::Text { content, font_size, color, max_width })
        }
        WidgetType::Button => {
            let label = r.read_string()?;
            let background = r.read_color()?;
            let text_color = r.read_color()?;
            let corner_radius = r.read_f32()?;
            Some(WidgetProperties::Button { label, background, text_color, corner_radius })
        }
        WidgetType::Image => {
            let path = r.read_string()?;
            Some(WidgetProperties::Image { path })
        }
        WidgetType::Toggle => {
            let is_on = r.read_u8()? == 1;
            let label = r.read_string()?;
            Some(WidgetProperties::Toggle { is_on, label })
        }
        WidgetType::TextField => {
            let placeholder = r.read_string()?;
            let value = r.read_string()?;
            Some(WidgetProperties::TextField { placeholder, value })
        }
        WidgetType::Divider => {
            let color = r.read_color()?;
            let thickness = r.read_f32()?;
            Some(WidgetProperties::Divider { color, thickness })
        }
        WidgetType::Spacer => {
            let min_length = r.read_f32()?;
            Some(WidgetProperties::Spacer { min_length })
        }
        WidgetType::Card => {
            let background = r.read_color()?;
            let corner_radius = r.read_f32()?;
            let milkiness = r.read_f32()?;
            Some(WidgetProperties::Card { background, corner_radius, milkiness })
        }
        WidgetType::VStack => {
            let spacing = r.read_f32()?;
            let alignment = r.read_u8()?;
            Some(WidgetProperties::VStack { spacing, alignment })
        }
        WidgetType::HStack => {
            let spacing = r.read_f32()?;
            let alignment = r.read_u8()?;
            Some(WidgetProperties::HStack { spacing, alignment })
        }
        WidgetType::ZStack => Some(WidgetProperties::ZStack),
        WidgetType::Padding => {
            let top = r.read_f32()?;
            let right = r.read_f32()?;
            let bottom = r.read_f32()?;
            let left = r.read_f32()?;
            Some(WidgetProperties::Padding { top, right, bottom, left })
        }
        WidgetType::Frame => {
            let width = r.read_optional_f32()?;
            let height = r.read_optional_f32()?;
            Some(WidgetProperties::Frame { width, height })
        }
    }
}

pub fn widget_tree_to_draw_commands(widgets: &[FlatWidget], offset_x: f32, offset_y: f32) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    for widget in widgets {
        let [x, y, w, h] = widget.bounds;
        let ax = x + offset_x;
        let ay = y + offset_y;
        match &widget.properties {
            WidgetProperties::Text { content, font_size, color, .. } => {
                commands.push(DrawCommand::Text { content: content.clone(), x: ax, y: ay + font_size, font_size: *font_size, color: *color, max_width: None });
            }
            WidgetProperties::Button { label, background, text_color, corner_radius } => {
                commands.push(DrawCommand::Rect { x: ax, y: ay, width: w, height: h, color: *background, corner_radius: *corner_radius });
                let text_x = ax + (w - label.len() as f32 * 8.0) * 0.5;
                let text_y = ay + (h - 13.0) * 0.5;
                commands.push(DrawCommand::Text { content: label.clone(), x: text_x, y: text_y, font_size: 13.0, color: *text_color, max_width: Some(w - 16.0) });
            }
            WidgetProperties::Card { background, corner_radius, milkiness } => {
                if *milkiness > 0.0 {
                    commands.push(DrawCommand::GlassPanel { x: ax, y: ay, width: w, height: h, milkiness: *milkiness, alpha: background.a, corner_radius: *corner_radius });
                } else {
                    commands.push(DrawCommand::Rect { x: ax, y: ay, width: w, height: h, color: *background, corner_radius: *corner_radius });
                }
            }
            WidgetProperties::Divider { color, thickness } => {
                commands.push(DrawCommand::Rect { x: ax, y: ay, width: w, height: *thickness, color: *color, corner_radius: 0.0 });
            }
            WidgetProperties::Toggle { is_on, .. } => {
                let track_color = if *is_on { Color::new(0.047, 0.522, 0.937, 1.0) } else { Color::new(0.35, 0.35, 0.37, 1.0) };
                let track_w = 42.0_f32.min(w);
                let track_h = 24.0_f32.min(h);
                commands.push(DrawCommand::Rect { x: ax, y: ay + (h - track_h) * 0.5, width: track_w, height: track_h, color: track_color, corner_radius: 12.0 });
                let thumb_x = if *is_on { ax + track_w - 9.0 - (track_h - 18.0) * 0.5 - 9.0 } else { ax + (track_h - 18.0) * 0.5 };
                commands.push(DrawCommand::Rect { x: thumb_x, y: ay + (h - 18.0) * 0.5, width: 18.0, height: 18.0, color: Color::WHITE, corner_radius: 9.0 });
            }
            WidgetProperties::TextField { placeholder, value } => {
                commands.push(DrawCommand::Rect { x: ax, y: ay, width: w, height: h, color: Color::new(0.15, 0.15, 0.17, 1.0), corner_radius: 6.0 });
                let display = if value.is_empty() { placeholder } else { value };
                let tc = if value.is_empty() { Color::new(0.45, 0.45, 0.47, 1.0) } else { Color::new(0.92, 0.92, 0.94, 1.0) };
                commands.push(DrawCommand::Text { content: display.clone(), x: ax + 8.0, y: ay + (h - 13.0) * 0.5, font_size: 13.0, color: tc, max_width: Some(w - 16.0) });
            }
            _ => {}
        }
    }
    commands
}

pub fn hit_test(widgets: &[FlatWidget], px: f32, py: f32) -> Option<usize> {
    for widget in widgets.iter().rev() {
        if !widget.interactive { continue; }
        let [x, y, w, h] = widget.bounds;
        if px >= x && px <= x + w && py >= y && py <= y + h {
            return Some(widget.node_id);
        }
    }
    None
}
