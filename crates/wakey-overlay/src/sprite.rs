//! Animated sprite for the Wakey overlay.
//!
//! Tabbie-style facial expressions with eyes, mouth, eyebrows, and accessories.

use eframe::egui::{Color32, Pos2, Vec2, Rect, Stroke, Painter};
use std::time::{Duration, Instant};

use crate::expressions::{Expression, EyeShape, MouthShape, EyebrowShape, Accessory};
use crate::animation_state::AnimationState;

#[derive(Debug, Clone)]
pub struct SpriteConfig {
    pub base_size: f32,
    pub breath_amplitude: f32,
    pub breath_speed: f32,
    pub eye_size: f32,
    pub show_accessories: bool,
}

impl Default for SpriteConfig {
    fn default() -> Self {
        Self {
            base_size: 40.0,
            breath_amplitude: 0.08,
            breath_speed: 0.5,
            eye_size: 1.0,
            show_accessories: true,
        }
    }
}

#[derive(Debug)]
pub struct Sprite {
    animation: AnimationState,
    config: SpriteConfig,
    last_update: Instant,
    breath_phase: f32,
    blink_state: BlinkState,
    next_blink: Instant,
}

#[derive(Debug)]
enum BlinkState {
    Open,
    Closing(Instant),
    Closed(Instant),
    Opening(Instant),
}

impl Sprite {
    pub fn new() -> Self {
        Self {
            animation: AnimationState::new(),
            config: SpriteConfig::default(),
            last_update: Instant::now(),
            breath_phase: 0.0,
            blink_state: BlinkState::Open,
            next_blink: Self::next_blink_time(),
        }
    }

    pub fn with_config(config: SpriteConfig) -> Self {
        Self {
            animation: AnimationState::new(),
            config,
            last_update: Instant::now(),
            breath_phase: 0.0,
            blink_state: BlinkState::Open,
            next_blink: Self::next_blink_time(),
        }
    }

    pub fn update(&mut self, now: Instant) -> bool {
        let dt = now.duration_since(self.last_update);
        self.last_update = now;

        let anim_changed = self.animation.update();

        let breath_delta = self.config.breath_speed * dt.as_secs_f32();
        self.breath_phase = (self.breath_phase + breath_delta) % 1.0;

        let blink_changed = self.update_blink(now);
        anim_changed || blink_changed
    }

    fn update_blink(&mut self, now: Instant) -> bool {
        match &self.blink_state {
            BlinkState::Open => {
                if now >= self.next_blink {
                    self.blink_state = BlinkState::Closing(now);
                    return true;
                }
            }
            BlinkState::Closing(start) => {
                if now.duration_since(*start) >= Duration::from_millis(60) {
                    self.blink_state = BlinkState::Closed(now);
                    return true;
                }
            }
            BlinkState::Closed(start) => {
                if now.duration_since(*start) >= Duration::from_millis(80) {
                    self.blink_state = BlinkState::Opening(now);
                    return true;
                }
            }
            BlinkState::Opening(start) => {
                if now.duration_since(*start) >= Duration::from_millis(60) {
                    self.blink_state = BlinkState::Open;
                    self.next_blink = Self::next_blink_time();
                    return true;
                }
            }
        }
        false
    }

    fn next_blink_time() -> Instant {
        let delay_ms = 2000 + (rand_simple() % 6000) as u64;
        Instant::now() + Duration::from_millis(delay_ms)
    }

    pub fn get_blink_factor(&self, now: Instant) -> f32 {
        match &self.blink_state {
            BlinkState::Open => 1.0,
            BlinkState::Closing(start) => {
                (1.0 - (now.duration_since(*start).as_secs_f32() / 0.06)).max(0.0)
            }
            BlinkState::Closed(_) => 0.0,
            BlinkState::Opening(start) => {
                (now.duration_since(*start).as_secs_f32() / 0.06).min(1.0)
            }
        }
    }

    pub fn set_expression(&mut self, expression: Expression) {
        self.animation.set_current(expression);
    }

    pub fn queue_expression(&mut self, expression: Expression) {
        self.animation.queue(expression);
    }

    pub fn current_expression(&self) -> &Expression {
        self.animation.current_expression()
    }

    pub fn reset(&mut self) {
        self.animation.reset();
    }

    pub fn draw(&self, painter: &Painter, center: Pos2) {
        let expr = self.current_expression();
        let now = Instant::now();
        let blink = self.get_blink_factor(now);

        let breath_scale = 1.0 + self.config.breath_amplitude 
            * (self.breath_phase * 2.0 * std::f32::consts::PI).sin();

        let size = self.config.base_size * breath_scale;

        self.draw_glow(painter, center, size, expr);
        self.draw_body(painter, center, size, expr);
        self.draw_eyes(painter, center, size, expr, blink);
        self.draw_eyebrows(painter, center, size, expr);
        self.draw_mouth(painter, center, size, expr);
        
        if self.config.show_accessories {
            self.draw_accessories(painter, center, size, expr);
        }
    }

    fn draw_glow(&self, p: &Painter, c: Pos2, s: f32, e: &Expression) {
        let [r, g, b, a] = e.glow_color;
        let base = Color32::from_rgba_unmultiplied(
            (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, (a * 255.0) as u8,
        );
        p.circle(c, s * 1.8, Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 30), Stroke::NONE);
        p.circle(c, s * 1.4, Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 80), Stroke::NONE);
        p.circle(c, s * 1.1, Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 150), Stroke::NONE);
    }

    fn draw_body(&self, p: &Painter, c: Pos2, s: f32, e: &Expression) {
        let [r, g, b, a] = e.glow_color;
        let body = Color32::from_rgba_unmultiplied(
            (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, (a * 255.0) as u8,
        );
        p.circle(c, s, body, Stroke::NONE);
        let hl = c + Vec2::new(-s * 0.2, -s * 0.2);
        p.circle(hl, s * 0.35, Color32::from_rgba_unmultiplied(255, 255, 240, 80), Stroke::NONE);
    }

    fn draw_eyes(&self, p: &Painter, c: Pos2, s: f32, e: &Expression, blink: f32) {
        let ex = s * 0.35;
        let ey = -s * 0.1;
        let sp = s * 0.25;
        let left = c + Vec2::new(-sp + ex, ey);
        let right = c + Vec2::new(sp + ex, ey);
        let ec = Color32::from_rgb(35, 30, 25);

        match e.eyes {
            EyeShape::Rectangle => {
                let w = s * 0.12 * self.config.eye_size * blink;
                let h = s * 0.16 * self.config.eye_size * blink;
                if w > 0.5 && h > 0.5 {
                    p.rect_filled(Rect::from_min_size(left - Vec2::new(w/2.0, h/2.0), Vec2::new(w, h)), 0.0, ec);
                    p.rect_filled(Rect::from_min_size(right - Vec2::new(w/2.0, h/2.0), Vec2::new(w, h)), 0.0, ec);
                }
            }
            EyeShape::Circle => {
                let r = s * 0.12 * self.config.eye_size * blink;
                if r > 0.5 {
                    p.circle(left, r, ec, Stroke::NONE);
                    p.circle(right, r, ec, Stroke::NONE);
                }
            }
            EyeShape::Wide => {
                let r = s * 0.18 * self.config.eye_size * blink;
                if r > 0.5 {
                    p.circle(left, r, ec, Stroke::NONE);
                    p.circle(right, r, ec, Stroke::NONE);
                }
            }
            EyeShape::Dot => {
                let r = s * 0.06 * self.config.eye_size * blink;
                if r > 0.5 {
                    p.circle(left, r, ec, Stroke::NONE);
                    p.circle(right, r, ec, Stroke::NONE);
                }
            }
            EyeShape::CurveUp => {
                if blink > 0.5 {
                    let st = Stroke::new(s * 0.04 * self.config.eye_size, ec);
                    p.line_segment([left - Vec2::new(s * 0.08, 0.0), left + Vec2::new(s * 0.08, -s * 0.05)], st);
                    p.line_segment([right - Vec2::new(s * 0.08, -s * 0.05), right + Vec2::new(s * 0.08, 0.0)], st);
                }
            }
            EyeShape::CurveDown => {
                if blink > 0.5 {
                    let st = Stroke::new(s * 0.04 * self.config.eye_size, ec);
                    p.line_segment([left - Vec2::new(s * 0.08, 0.0), left + Vec2::new(s * 0.08, s * 0.05)], st);
                    p.line_segment([right - Vec2::new(s * 0.08, s * 0.05), right + Vec2::new(s * 0.08, 0.0)], st);
                }
            }
            EyeShape::Wink => {
                let r = s * 0.12 * self.config.eye_size * blink;
                if r > 0.5 {
                    p.circle(left, r, ec, Stroke::NONE);
                }
                let st = Stroke::new(s * 0.04 * self.config.eye_size, ec);
                p.line_segment([right - Vec2::new(s * 0.08, 0.0), right + Vec2::new(s * 0.08, 0.0)], st);
            }
        }

        // Highlights
        if blink > 0.3 && !matches!(e.eyes, EyeShape::CurveUp | EyeShape::CurveDown) {
            let hr = s * 0.04 * self.config.eye_size;
            let ho = Vec2::new(-s * 0.05, -s * 0.05);
            let hc = Color32::from_rgba_unmultiplied(255, 255, 255, 180);
            p.circle(left + ho, hr, hc, Stroke::NONE);
            p.circle(right + ho, hr, hc, Stroke::NONE);
        }
    }

    fn draw_eyebrows(&self, p: &Painter, c: Pos2, s: f32, e: &Expression) {
        if e.eyebrows == EyebrowShape::None { return; }

        let by = c.y - s * 0.35;
        let bx = s * 0.25;
        let bl = s * 0.18;
        let st = Stroke::new(s * 0.04, Color32::from_rgb(35, 30, 25));

        match e.eyebrows {
            EyebrowShape::Angry => {
                p.line_segment([c + Vec2::new(-bx - bl/2.0, by - s * 0.05), c + Vec2::new(-bx + bl/2.0, by + s * 0.05)], st);
                p.line_segment([c + Vec2::new(bx - bl/2.0, by + s * 0.05), c + Vec2::new(bx + bl/2.0, by - s * 0.05)], st);
            }
            EyebrowShape::Worried => {
                p.line_segment([c + Vec2::new(-bx - bl/2.0, by + s * 0.05), c + Vec2::new(-bx + bl/2.0, by - s * 0.05)], st);
                p.line_segment([c + Vec2::new(bx - bl/2.0, by - s * 0.05), c + Vec2::new(bx + bl/2.0, by + s * 0.05)], st);
            }
            EyebrowShape::None => {}
        }
    }

    fn draw_mouth(&self, p: &Painter, c: Pos2, s: f32, e: &Expression) {
        let my = c.y + s * 0.25;
        let mc = Color32::from_rgb(35, 30, 25);
        let st = Stroke::new(s * 0.04, mc);

        match e.mouth {
            MouthShape::None => {}
            MouthShape::Smile => {
                p.line_segment([c + Vec2::new(-s * 0.12, my), c + Vec2::new(0.0, my + s * 0.08)], st);
                p.line_segment([c + Vec2::new(0.0, my + s * 0.08), c + Vec2::new(s * 0.12, my)], st);
            }
            MouthShape::Flat => {
                p.line_segment([c + Vec2::new(-s * 0.1, my), c + Vec2::new(s * 0.1, my)], st);
            }
            MouthShape::Open => {
                p.circle(c + Vec2::new(0.0, my), s * 0.08, Color32::from_rgb(50, 40, 35), Stroke::NONE);
            }
            MouthShape::Wavy => {
                let pts = [
                    c + Vec2::new(-s * 0.12, my),
                    c + Vec2::new(-s * 0.06, my + s * 0.03),
                    c + Vec2::new(0.0, my),
                    c + Vec2::new(s * 0.06, my + s * 0.03),
                    c + Vec2::new(s * 0.12, my),
                ];
                for w in pts.windows(2) {
                    p.line_segment([w[0], w[1]], st);
                }
            }
            MouthShape::Tongue => {
                p.line_segment([c + Vec2::new(-s * 0.12, my), c + Vec2::new(0.0, my + s * 0.08)], st);
                p.line_segment([c + Vec2::new(0.0, my + s * 0.08), c + Vec2::new(s * 0.12, my)], st);
                let tc = Color32::from_rgb(220, 100, 120);
                p.circle(c + Vec2::new(0.0, my + s * 0.12), s * 0.05, tc, Stroke::NONE);
            }
        }
    }

    fn draw_accessories(&self, p: &Painter, c: Pos2, s: f32, e: &Expression) {
        for acc in &e.accessories {
            match acc {
                Accessory::Lightbulb => self.draw_lightbulb(p, c, s),
                Accessory::PointingLeft => self.draw_pointing(p, c, s, true),
                Accessory::PointingRight => self.draw_pointing(p, c, s, false),
                Accessory::ThinkingHand => self.draw_thinking(p, c, s),
                Accessory::CoffeeCup => self.draw_coffee(p, c, s),
                Accessory::Zzz => self.draw_zzz(p, c, s),
                Accessory::Heart => self.draw_heart(p, c, s),
                Accessory::Sparkle => self.draw_sparkle(p, c, s),
            }
        }
    }

    fn draw_lightbulb(&self, p: &Painter, c: Pos2, s: f32) {
        let x = c.x + s * 1.3;
        let y = c.y - s * 0.8;
        let r = s * 0.25;
        p.circle(Pos2::new(x, y), r, Color32::from_rgb(255, 220, 50), Stroke::NONE);
        p.rect_filled(Rect::from_min_size(Pos2::new(x - r * 0.4, y + r * 0.3), Vec2::new(r * 0.8, r * 0.5)), 0.0, Color32::from_rgb(150, 150, 150));
    }

    fn draw_pointing(&self, p: &Painter, c: Pos2, s: f32, left: bool) {
        let x = c.x + if left { -s * 1.4 } else { s * 1.4 };
        let y = c.y + s * 0.2;
        let w = s * 0.15;
        let h = s * 0.35;
        p.rect_filled(Rect::from_min_size(Pos2::new(x - w/2.0, y), Vec2::new(w, h)), 2.0, Color32::from_rgb(255, 230, 200));
    }

    fn draw_thinking(&self, p: &Painter, c: Pos2, s: f32) {
        p.circle(c + Vec2::new(s * 0.6, s * 0.5), s * 0.2, Color32::from_rgb(255, 230, 200), Stroke::NONE);
    }

    fn draw_coffee(&self, p: &Painter, c: Pos2, s: f32) {
        let x = c.x;
        let y = c.y + s * 1.2;
        let w = s * 0.35;
        let h = s * 0.3;
        p.rect_filled(Rect::from_min_size(Pos2::new(x - w/2.0, y), Vec2::new(w, h)), 3.0, Color32::from_rgb(200, 150, 100));
    }

    fn draw_zzz(&self, p: &Painter, c: Pos2, s: f32) {
        let x = c.x + s * 1.0;
        let y = c.y - s * 0.8;
        let st = Stroke::new(s * 0.04, Color32::from_rgb(180, 180, 255));
        p.line_segment([Pos2::new(x - s * 0.1, y), Pos2::new(x + s * 0.1, y)], st);
        p.line_segment([Pos2::new(x + s * 0.1, y), Pos2::new(x - s * 0.1, y + s * 0.15)], st);
        p.line_segment([Pos2::new(x - s * 0.1, y + s * 0.15), Pos2::new(x + s * 0.1, y + s * 0.15)], st);
    }

    fn draw_heart(&self, p: &Painter, c: Pos2, s: f32) {
        let x = c.x + s * 1.2;
        let y = c.y - s * 0.5;
        let hs = s * 0.2;
        let col = Color32::from_rgb(255, 80, 120);
        p.circle(Pos2::new(x - hs * 0.3, y - hs * 0.1), hs * 0.35, col, Stroke::NONE);
        p.circle(Pos2::new(x + hs * 0.3, y - hs * 0.1), hs * 0.35, col, Stroke::NONE);
        p.circle(Pos2::new(x, y + hs * 0.2), hs * 0.4, col, Stroke::NONE);
    }

    fn draw_sparkle(&self, p: &Painter, c: Pos2, s: f32) {
        let col = Color32::from_rgb(255, 255, 200);
        for i in 0..4 {
            let angle = (i as f32 / 4.0) * std::f32::consts::TAU;
            let x = c.x + angle.cos() * s * 1.5;
            let y = c.y + angle.sin() * s * 1.5;
            p.circle(Pos2::new(x, y), s * 0.08, col, Stroke::NONE);
        }
    }
}

impl Default for Sprite {
    fn default() -> Self {
        Self::new()
    }
}

fn rand_simple() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let val = COUNTER.fetch_add(1, Ordering::Relaxed);
    (val.wrapping_mul(1103515245).wrapping_add(12345)) % 6000
}
