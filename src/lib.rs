#![allow(non_snake_case)]

mod CustomVerticalSlider;
mod biquad_filters;
mod db_meter;
mod ui_knob;
use atomic_float::AtomicF32;
use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Color32, FontId, Rect, RichText, Rounding, Ui},
    EguiState,
};
use std::{
    ops::RangeInclusive,
    sync::{Arc, Mutex},
};
use CustomVerticalSlider::ParamSlider as VerticalParamSlider;
use biquad_filters::FilterType;

/**************************************************
 * Fawn Faders by Ardura
 *
 * Build with: cargo xtask bundle FawnFaders
 * ************************************************/

// GUI Colors
const WHITE: Color32 = Color32::from_rgb(242, 243, 244);
const GRAY: Color32 = Color32::from_rgb(78, 81, 90);
const BLACK: Color32 = Color32::from_rgb(4, 7, 14);
const TEAL: Color32 = Color32::from_rgb(110, 201, 235);
const DARKTEAL: Color32 = Color32::from_rgb(37, 68, 90);

// Plugin sizing
const WIDTH: u32 = 370;
const HEIGHT: u32 = 720;

// Constants
const VERT_BAR_HEIGHT: f32 = 260.0;
const VERT_BAR_WIDTH: f32 = 32.0;

/// The time it takes for the peak meter to decay by 12 dB after switching to complete silence.
const PEAK_METER_DECAY_MS: f64 = 100.0;

const MAIN_FONT: nih_plug_egui::egui::FontId = FontId::monospace(8.0);

#[derive(Clone, Copy)]
struct EQ {
    bands: [biquad_filters::Biquad; 1],
}

pub struct FawnFaders {
    params: Arc<FawnFadersParams>,

    // normalize the peak meter's response based on the sample rate with this
    out_meter_decay_weight: f32,

    // Equalizer made of peaks
    equalizer: Arc<Mutex<EQ>>,

    // The current data for the different meters
    out_meter: Arc<AtomicF32>,
    in_meter: Arc<AtomicF32>,
}

#[derive(Params)]
struct FawnFadersParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "input_gain"]
    pub input_gain: FloatParam,

    #[id = "output_gain"]
    pub output_gain: FloatParam,

    #[id = "dry_wet"]
    pub dry_wet: FloatParam,

    #[id = "oversampling"]
    pub oversampling: FloatParam,

    // Bands
    #[id = "freq_band_0"]
    pub freq_band_0: FloatParam,

    #[id = "freq_band_1"]
    pub freq_band_1: FloatParam,

    #[id = "freq_band_2"]
    pub freq_band_2: FloatParam,

    #[id = "freq_band_3"]
    pub freq_band_3: FloatParam,

    #[id = "freq_band_4"]
    pub freq_band_4: FloatParam,

    // Gain
    #[id = "gain_band_0"]
    pub gain_band_0: FloatParam,

    #[id = "gain_band_1"]
    pub gain_band_1: FloatParam,

    #[id = "gain_band_2"]
    pub gain_band_2: FloatParam,

    #[id = "gain_band_3"]
    pub gain_band_3: FloatParam,

    #[id = "gain_band_4"]
    pub gain_band_4: FloatParam,

    // Resonance
    #[id = "res_band_0"]
    pub res_band_0: FloatParam,

    #[id = "res_band_1"]
    pub res_band_1: FloatParam,

    #[id = "res_band_2"]
    pub res_band_2: FloatParam,

    #[id = "res_band_3"]
    pub res_band_3: FloatParam,

    #[id = "res_band_4"]
    pub res_band_4: FloatParam,
}

impl Default for FawnFaders {
    fn default() -> Self {
        Self {
            params: Arc::new(FawnFadersParams::default()),
            out_meter_decay_weight: 1.0,
            out_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
            in_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
            // Hard code to 44100, will update in processing
            equalizer: Arc::new(Mutex::new(EQ {
                bands: [
                        // These defaults don't matter as they are overwritten immediately
                        biquad_filters::Biquad::new( 44100.0,800.0,0.0, 0.707, FilterType::Peak)
                        // 5 Bands of the above
                        ; 1
                    ],
            })),
        }
    }
}

impl Default for FawnFadersParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(WIDTH, HEIGHT),

            // Input gain dB parameter
            input_gain: FloatParam::new(
                "In",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-12.0),
                    max: util::db_to_gain(12.0),
                    factor: FloatRange::gain_skew_factor(-12.0, 12.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // Output gain parameter
            output_gain: FloatParam::new(
                "Out",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-12.0),
                    max: util::db_to_gain(12.0),
                    factor: FloatRange::gain_skew_factor(-12.0, 12.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            // Dry/Wet parameter
            dry_wet: FloatParam::new("Wet", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit("%")
                .with_value_to_string(formatters::v2s_f32_percentage(2))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            oversampling: FloatParam::new(
                "Oversample",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 2.0,
                },
            )
            .with_step_size(1.0),

            // Non Param Buttons
            freq_band_0: FloatParam::new(
                "Band 0",
                120.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20000.0,
                    factor: 0.5,
                },
            )
            .with_step_size(1.0)
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_value_to_string(formatters::v2s_f32_hz_then_khz_with_note_name(2, false)),
            freq_band_1: FloatParam::new(
                "Band 1",
                360.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20000.0,
                    factor: 0.5,
                },
            )
            .with_step_size(1.0)
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_value_to_string(formatters::v2s_f32_hz_then_khz_with_note_name(2, false)),
            freq_band_2: FloatParam::new(
                "Band 2",
                1200.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20000.0,
                    factor: 0.5,
                },
            )
            .with_step_size(1.0)
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_value_to_string(formatters::v2s_f32_hz_then_khz_with_note_name(2, false)),
            freq_band_3: FloatParam::new(
                "Band 3",
                5000.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20000.0,
                    factor: 0.5,
                },
            )
            .with_step_size(1.0)
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_value_to_string(formatters::v2s_f32_hz_then_khz_with_note_name(2, false)),
            freq_band_4: FloatParam::new(
                "Band 4",
                12000.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20000.0,
                    factor: 0.5,
                },
            )
            .with_step_size(1.0)
            .with_smoother(SmoothingStyle::Linear(5.0))
            .with_value_to_string(formatters::v2s_f32_hz_then_khz_with_note_name(2, false)),

            // Gain Bands
            gain_band_0: FloatParam::new(
                "Gain 0",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            gain_band_1: FloatParam::new(
                "Gain 1",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            gain_band_2: FloatParam::new(
                "Gain 2",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            gain_band_3: FloatParam::new(
                "Gain 3",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            gain_band_4: FloatParam::new(
                "Gain 4",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            // Res Bands
            res_band_0: FloatParam::new(
                "Res 0",
                0.707,
                FloatRange::Linear {
                    min: 0.01,
                    max: 1.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            res_band_1: FloatParam::new(
                "Res 1",
                0.707,
                FloatRange::Linear {
                    min: 0.01,
                    max: 1.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            res_band_2: FloatParam::new(
                "Res 2",
                0.707,
                FloatRange::Linear {
                    min: 0.01,
                    max: 1.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            res_band_3: FloatParam::new(
                "Res 3",
                0.707,
                FloatRange::Linear {
                    min: 0.01,
                    max: 1.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            res_band_4: FloatParam::new(
                "Res 4",
                0.707,
                FloatRange::Linear {
                    min: 0.01,
                    max: 1.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
        }
    }
}

impl FawnFaders {
    fn create_band_gui(
        ui: &mut Ui,
        freq_param: &FloatParam,
        gain_param: &FloatParam,
        res_param: &FloatParam,
        setter: &ParamSetter<'_>,
        knob_size: f32,
    ) {
        ui.vertical(|ui| {
            ui.add(
                VerticalParamSlider::for_param(gain_param, setter)
                    .with_width(VERT_BAR_WIDTH * 2.0)
                    .with_height(VERT_BAR_HEIGHT)
                    .set_reversed(true),
            );
            let mut freq_knob = ui_knob::ArcKnob::for_param(freq_param, setter, knob_size);
            freq_knob.preset_style(ui_knob::KnobStyle::NewPresets2);
            freq_knob.set_fill_color(TEAL);
            freq_knob.set_line_color(GRAY);
            freq_knob.set_show_label(true);
            freq_knob.set_text_size(10.0);
            ui.add(freq_knob);

            let mut res_knob = ui_knob::ArcKnob::for_param(res_param, setter, knob_size);
            res_knob.preset_style(ui_knob::KnobStyle::NewPresets2);
            res_knob.set_fill_color(TEAL);
            res_knob.set_line_color(GRAY);
            res_knob.set_show_label(true);
            res_knob.set_text_size(10.0);
            ui.add(res_knob);
        });
    }
}

impl Plugin for FawnFaders {
    const NAME: &'static str = "Fawn Faders";
    const VENDOR: &'static str = "Ardura";
    const URL: &'static str = "https://github.com/ardura";
    const EMAIL: &'static str = "azviscarra@gmail.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // This looks like it's flexible for running the plugin in mono or stereo
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let in_meter = self.in_meter.clone();
        let out_meter = self.out_meter.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, setter, _state| {
                egui::CentralPanel::default().show(egui_ctx, |ui| {
                    // Assign default colors
                    ui.style_mut().visuals.widgets.inactive.bg_stroke.color = BLACK;
                    ui.style_mut().visuals.widgets.inactive.bg_fill = BLACK;
                    ui.style_mut().visuals.widgets.active.fg_stroke.color = TEAL;
                    ui.style_mut().visuals.widgets.active.bg_stroke.color = TEAL;
                    ui.style_mut().visuals.widgets.open.fg_stroke.color = TEAL;
                    ui.style_mut().visuals.widgets.open.bg_fill = GRAY;
                    // Lettering on param sliders
                    ui.style_mut().visuals.widgets.inactive.fg_stroke.color = TEAL;
                    // Background of the bar in param sliders
                    ui.style_mut().visuals.selection.bg_fill = TEAL;
                    ui.style_mut().visuals.selection.stroke.color = TEAL;
                    // Unfilled background of the bar
                    ui.style_mut().visuals.widgets.noninteractive.bg_fill = GRAY;

                    // Set default font
                    ui.style_mut().override_font_id = Some(MAIN_FONT);

                    // Trying to draw background colors as rects
                    ui.painter().rect_filled(
                        Rect::from_x_y_ranges(
                            RangeInclusive::new(0.0, WIDTH as f32),
                            RangeInclusive::new(0.0, HEIGHT as f32),
                        ),
                        Rounding::none(),
                        BLACK,
                    );

                    // GUI Structure
                    ui.vertical(|ui| {
                        // Spacing :)
                        ui.label(
                            RichText::new(" Fawn Faders")
                                .font(FontId::proportional(14.0))
                                .color(WHITE),
                        )
                        .on_hover_text("by Ardura!");

                        // Peak Meters
                        let in_meter =
                            util::gain_to_db(in_meter.load(std::sync::atomic::Ordering::Relaxed));
                        let in_meter_text = if in_meter > util::MINUS_INFINITY_DB {
                            format!("{in_meter:.1} dBFS Input")
                        } else {
                            String::from("-inf dBFS Input")
                        };
                        let in_meter_normalized = (in_meter + 60.0) / 60.0;
                        ui.allocate_space(egui::Vec2::splat(2.0));
                        let mut in_meter_obj =
                            db_meter::DBMeter::new(in_meter_normalized).text(in_meter_text);
                        in_meter_obj.set_background_color(GRAY);
                        in_meter_obj.set_bar_color(WHITE);
                        in_meter_obj.set_border_color(BLACK);
                        ui.add(in_meter_obj);

                        let out_meter =
                            util::gain_to_db(out_meter.load(std::sync::atomic::Ordering::Relaxed));
                        let out_meter_text = if out_meter > util::MINUS_INFINITY_DB {
                            format!("{out_meter:.1} dBFS Output")
                        } else {
                            String::from("-inf dBFS Output")
                        };
                        let out_meter_normalized = (out_meter + 60.0) / 60.0;
                        ui.allocate_space(egui::Vec2::splat(2.0));
                        let mut out_meter_obj =
                            db_meter::DBMeter::new(out_meter_normalized).text(out_meter_text);
                        out_meter_obj.set_background_color(GRAY);
                        out_meter_obj.set_bar_color(TEAL);
                        out_meter_obj.set_border_color(DARKTEAL);
                        ui.add(out_meter_obj);

                        ui.separator();

                        // UI Control area
                        egui::scroll_area::ScrollArea::horizontal()
                            .auto_shrink([true; 2])
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Draw our band UI
                                    Self::create_band_gui(
                                        ui,
                                        &params.freq_band_0,
                                        &params.gain_band_0,
                                        &params.res_band_0,
                                        setter,
                                        VERT_BAR_WIDTH,
                                    );
                                    Self::create_band_gui(
                                        ui,
                                        &params.freq_band_1,
                                        &params.gain_band_1,
                                        &params.res_band_1,
                                        setter,
                                        VERT_BAR_WIDTH,
                                    );
                                    Self::create_band_gui(
                                        ui,
                                        &params.freq_band_2,
                                        &params.gain_band_2,
                                        &params.res_band_2,
                                        setter,
                                        VERT_BAR_WIDTH,
                                    );
                                    Self::create_band_gui(
                                        ui,
                                        &params.freq_band_3,
                                        &params.gain_band_3,
                                        &params.res_band_3,
                                        setter,
                                        VERT_BAR_WIDTH,
                                    );
                                    Self::create_band_gui(
                                        ui,
                                        &params.freq_band_4,
                                        &params.gain_band_4,
                                        &params.res_band_4,
                                        setter,
                                        VERT_BAR_WIDTH,
                                    );
                                });
                            });

                        ui.horizontal(|ui| {
                            let mut os_knob = ui_knob::ArcKnob::for_param(
                                &params.oversampling,
                                setter,
                                VERT_BAR_WIDTH,
                            );
                            os_knob.preset_style(ui_knob::KnobStyle::NewPresets2);
                            os_knob.set_text_size(10.0);
                            os_knob.set_fill_color(TEAL);
                            os_knob.set_line_color(WHITE);
                            ui.add(os_knob);

                            let mut gain_knob = ui_knob::ArcKnob::for_param(
                                &params.input_gain,
                                setter,
                                VERT_BAR_WIDTH,
                            );
                            gain_knob.preset_style(ui_knob::KnobStyle::NewPresets2);
                            gain_knob.set_text_size(10.0);
                            gain_knob.set_fill_color(TEAL);
                            gain_knob.set_line_color(WHITE);
                            ui.add(gain_knob);

                            let mut output_knob = ui_knob::ArcKnob::for_param(
                                &params.output_gain,
                                setter,
                                VERT_BAR_WIDTH,
                            );
                            output_knob.preset_style(ui_knob::KnobStyle::NewPresets2);
                            output_knob.set_text_size(10.0);
                            output_knob.set_fill_color(TEAL);
                            output_knob.set_line_color(WHITE);
                            ui.add(output_knob);

                            let mut dry_wet_knob = ui_knob::ArcKnob::for_param(
                                &params.dry_wet,
                                setter,
                                VERT_BAR_WIDTH,
                            );
                            dry_wet_knob.preset_style(ui_knob::KnobStyle::NewPresets2);
                            dry_wet_knob.set_text_size(10.0);
                            dry_wet_knob.set_fill_color(TEAL);
                            dry_wet_knob.set_line_color(WHITE);
                            ui.add(dry_wet_knob);
                        });
                    });
                });
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // After `PEAK_METER_DECAY_MS` milliseconds of pure silence, the peak meter's value should
        // have dropped by 12 dB
        self.out_meter_decay_weight = 0.25f64
            .powf((buffer_config.sample_rate as f64 * PEAK_METER_DECAY_MS / 1000.0).recip())
            as f32;

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let arc_eq = self.equalizer.clone();
        for mut channel_samples in buffer.iter_samples() {
            let mut out_amplitude = 0.0;
            let mut in_amplitude = 0.0;
            let mut processed_sample_l: f32 = 0.0;
            let mut processed_sample_r: f32 = 0.0;
            let num_samples = channel_samples.len();

            let gain = util::gain_to_db(self.params.input_gain.smoothed.next());
            let output_gain = self.params.output_gain.smoothed.next();
            let dry_wet = self.params.dry_wet.value();

            // Split left and right same way original subhoofer did
            let mut in_l: f32 = *channel_samples.get_mut(0).unwrap();
            let mut in_r: f32 = *channel_samples.get_mut(1).unwrap();

            // Make sure we are always on the correct sample rate, then update our EQ
            let mut eq = arc_eq.lock().unwrap();

            let sr = _context.transport().sample_rate;

            // Apply our input gain to our incoming signal
            in_l *= util::db_to_gain(gain);
            in_r *= util::db_to_gain(gain);

            // Calculate our amplitude for the decibel meter
            in_amplitude += in_l + in_r;

            //let os = self.params.oversampling.value() as i32;
            eq.bands[0].update(
                sr,
                self.params.freq_band_0.value(),
                self.params.gain_band_0.value(),
                self.params.res_band_0.value(),
            );
            //eq.bands[1].update(sr, self.params.freq_band_1.value(), self.params.gain_band_1.value(), self.params.res_band_1.value());
            //eq.bands[2].update(sr, self.params.freq_band_2.value(), self.params.gain_band_2.value(), self.params.res_band_2.value());
            //eq.bands[3].update(sr, self.params.freq_band_3.value(), self.params.gain_band_3.value(), self.params.res_band_3.value());
            //eq.bands[4].update(sr, self.params.freq_band_4.value(), self.params.gain_band_4.value(), self.params.res_band_4.value());

            // Perform processing on the sample using the filters
            for filter in eq.bands.iter_mut() {
                let mut temp_l: f32 = 0.0;
                let mut temp_r: f32 = 0.0;
                for i in 0..=self.params.oversampling.value() as usize {
                    match i {
                        0 => {
                            (temp_l, temp_r) = filter.process_sample(in_l, in_r);
                        },
                        _ => {
                            (temp_l, temp_r) = filter.process_sample(temp_l, temp_r);
                        }
                    }
                    
                }

                // Sum up our output
                processed_sample_l += temp_l;
                processed_sample_r += temp_r;
            }
            // Calculate dry/wet mix
            let wet_gain = dry_wet;
            let dry_gain = 1.0 - dry_wet;
            processed_sample_l = in_l * dry_gain + processed_sample_l * wet_gain;
            processed_sample_r = in_r * dry_gain + processed_sample_r * wet_gain;

            // Output gain
            processed_sample_l *= output_gain;
            processed_sample_r *= output_gain;

            // Assign back so we can output our processed sounds
            *channel_samples.get_mut(0).unwrap() = processed_sample_l;
            *channel_samples.get_mut(1).unwrap() = processed_sample_r;

            out_amplitude += processed_sample_l + processed_sample_r;

            // To save resources, a plugin can (and probably should!) only perform expensive
            // calculations that are only displayed on the GUI while the GUI is open
            if self.params.editor_state.is_open() {
                // Input gain meter
                in_amplitude = (in_amplitude / num_samples as f32).abs();
                let current_in_meter = self.in_meter.load(std::sync::atomic::Ordering::Relaxed);
                let new_in_meter = if in_amplitude > current_in_meter {
                    in_amplitude
                } else {
                    current_in_meter * self.out_meter_decay_weight
                        + in_amplitude * (1.0 - self.out_meter_decay_weight)
                };
                self.in_meter
                    .store(new_in_meter, std::sync::atomic::Ordering::Relaxed);

                // Output gain meter
                out_amplitude = (out_amplitude / num_samples as f32).abs();
                let current_out_meter = self.out_meter.load(std::sync::atomic::Ordering::Relaxed);
                let new_out_meter = if out_amplitude > current_out_meter {
                    out_amplitude
                } else {
                    current_out_meter * self.out_meter_decay_weight
                        + out_amplitude * (1.0 - self.out_meter_decay_weight)
                };
                self.out_meter
                    .store(new_out_meter, std::sync::atomic::Ordering::Relaxed);
            }
        }
        ProcessStatus::Normal
    }

    const MIDI_INPUT: MidiConfig = MidiConfig::None;

    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const HARD_REALTIME_ONLY: bool = false;

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        // In the default implementation we can simply ignore the value
        Box::new(|_| ())
    }

    fn filter_state(_state: &mut PluginState) {}

    fn reset(&mut self) {}

    fn deactivate(&mut self) {}
}

impl ClapPlugin for FawnFaders {
    const CLAP_ID: &'static str = "com.ardura.fawnfaders";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("An EQ");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
        ClapFeature::Equalizer,
    ];
}

impl Vst3Plugin for FawnFaders {
    const VST3_CLASS_ID: [u8; 16] = *b"FawnFadersAAAAAA";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Eq];
}

nih_export_clap!(FawnFaders);
nih_export_vst3!(FawnFaders);
