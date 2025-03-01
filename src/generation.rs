use crate::{stats, stats::{bigram_stats, layout_raw_to_table}, Stats, Finger};
use ahash::AHashMap;
use indicatif::MultiProgress;
use indicatif::ProgressBar;
use rand::prelude::*;
use rand::seq::SliceRandom;
use std::thread;
const THREADS: usize = 8;

#[must_use]
pub fn generate_threads(
    layout_raw: [char; 32],
    corpus: &String,
    max_iterations: u64,
    magic_rules: usize,
    cooling_rate: f64,
) -> ([char; 32], f64, Vec<String>) {
    let mut layouts: [([char; 32], f64, Vec<String>); THREADS] = Default::default();
    let bars = MultiProgress::new();
    thread::scope(|s| {
        let vec: Vec<_> = (0..THREADS)
            .map(|_| {
                s.spawn(|| {
                    generate(
                        layout_raw,
                        corpus,
                        max_iterations,
                        &bars,
                        magic_rules,
                        cooling_rate,
                    )
                })
            })
            .collect();
        for (i, handle) in vec.into_iter().enumerate() {
            let res = handle.join();
            layouts[i] = Result::expect(res, "idk");
        }
    });
    compare_layouts(&layouts)
}

fn generate(
    layout_raw: [char; 32],
    corpus: &String,
    max_iterations: u64,
    multibars: &MultiProgress,
    magic_rules: usize,
    cooling_rate: f64,
) -> ([char; 32], f64, Vec<String>) {
    let mut rng = thread_rng();
    let mut layout: ([char; 32], Stats, Vec<String>) = (
        layout_raw,
        Stats::default(),
        vec![String::default(); magic_rules],
    );
    layout.0.shuffle(&mut rng);
    layout.1 = stats::analyze(
        corpus.to_string(),
        layout.0,
        &"generate".to_string(),
        &layout.2,
    );
    let bar = ProgressBar::new(max_iterations);
    multibars.add(bar.clone());
    let stat_array: &[Stats; 10] = &Default::default();
    for ref mut layout_stats in stat_array {
        let mut rng = rand::thread_rng();
        let letter1 = rng.gen_range(0..layout.0.len());
        let letter2 = rng.gen_range(0..layout.0.len());
        layout.0.swap(letter1, letter2);
        layout.1 = stats::analyze(
            corpus.to_string(),
            layout.0,
            &"generate".to_string(),
            &layout.2,
        );
        *layout_stats = &layout.1.clone();
    }
    let mut temparature = standard_deviation(&stat_array.clone());
    let mut iterations = 0;
    while iterations < max_iterations {
        iterations += 1;
        layout = attempt_swap(
            layout.0,
            corpus,
            layout.1.clone(),
            layout.2,
            &layout.1.bad_bigrams,
            temparature,
            magic_rules,
        );
        bar.inc(1);
        temparature *= cooling_rate;
    }
    (layout.0, layout.1.score, layout.2)
}

pub fn attempt_swap(
    layout: [char; 32],
    corpus: &String,
    old_stats: Stats,
    old_magic: Vec<String>,
    bad_bigrams: &AHashMap<[char; 2], u32>,
    temparature: f64,
    magic_rules: usize
) -> ([char; 32], Stats, Vec<String>) {
    let mut rng = rand::thread_rng();
    let letter1 = rng.gen_range(0..layout.len());
    let letter2 = rng.gen_range(0..layout.len());

    let mut new_layout = layout;
    if rng.gen_range(0..10) > 3 {
        new_layout.swap(letter1, letter2);
    } else {
        new_layout = column_swap(new_layout, rng.gen_range(1..10), rng.gen_range(1..10));
    }

    let magic = get_magic_rules(&corpus.to_string(), new_layout, &"generate".to_string(), magic_rules);

    let new_stats = stats::analyze(
        corpus.to_string(),
        new_layout,
        &"generate".to_string(),
        &magic,
    );

    if new_stats.score > old_stats.score
        || annealing_func(old_stats.score, new_stats.score, temparature)
    {
        (new_layout, new_stats, magic)
    } else {
        (layout, old_stats, old_magic)
    }
}

fn compare_layouts(
    layouts: &[([char; 32], f64, Vec<String>); THREADS],
) -> ([char; 32], f64, Vec<String>) {
    let mut best_score = layouts[0].1;
    let mut best_layout = 0;
    for (i, item) in layouts.iter().enumerate() {
        if item.1 > best_score {
            best_layout = i;
            best_score = item.1;
        }
    }
    layouts[best_layout].clone()
}

fn standard_deviation(stat_array: &[Stats; 10]) -> f64 {
    let mut sum = 0.0;
    for layout in stat_array {
        sum += layout.score;
    }
    let mean = sum / stat_array.len() as f64;
    sum = 0.0;
    for layout in stat_array {
        sum += (layout.score - mean).powf(2.0);
    }
    let variance = sum / stat_array.len() as f64;
    variance.sqrt()
}
fn annealing_func(old: f64, new: f64, temparature: f64) -> bool {
    let mut rng = ThreadRng::default();
    let delta: f64 = new - old;
    let probability = 1.0 / (1.0 + (delta / temparature).exp());
    rng.gen_range(0.0..1.0) > probability
}

fn column_swap(mut layout: [char; 32], col1: usize, col2: usize) -> [char; 32] {
    layout.swap(col1, col2);
    layout.swap(col1 + 10, col2 + 10);
    layout.swap(col1 + 20, col2 + 20);
    layout
}

fn get_magic_rules(corpus: &String, layout_letters: [char; 32], command: &String, magic_rules: usize) -> Vec<String> {
    let layout = layout_raw_to_table(&layout_letters);
    let mut previous_letter= '⎵';
    let mut stats: Stats = Stats::default();
    let mut char_freq: AHashMap<char, u32> = AHashMap::default();
    let finger_weights: AHashMap<Finger, i64> = AHashMap::from([
        (Finger::Pinky, 66),
        (Finger::Ring, 28),
        (Finger::Middle, 21),
        (Finger::Index, 18),
        (Finger::Thumb, 50),
    ]);

    for letter in corpus.chars() {
        let key = &layout[&letter];
        let previous_key = &layout[&previous_letter];
        let bigram = bigram_stats::bigram_stats(previous_key, key, command, stats, &finger_weights);
        stats = bigram.0;
        if bigram.2 {
            *stats
                .bad_bigrams
                .entry([previous_letter, letter])
                .or_insert(0) += bigram.3;
        }
        previous_letter = letter;
    }

    let mut sorted_vec: Vec<([char; 2], u32)> = stats.bad_bigrams.into_iter().collect();
    sorted_vec.sort_by(|a, b| b.1.cmp(&a.1));

    // Extract only the keys ([char; 2])
    let sorted_keys: Vec<String> = sorted_vec.into_iter().take(magic_rules).map(|(key, _)| key.iter().collect::<String>()).collect();
    return sorted_keys;
}
