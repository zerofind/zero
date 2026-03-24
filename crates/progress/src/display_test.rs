//! Tests for ProgressDisplay

use super::display::ProgressDisplay;
use super::tracker::AtomicProgress;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_display_start_stop() {
    let progress = Arc::new(AtomicProgress::new(10, 1000));
    let display = ProgressDisplay::builder(progress).start();

    // Just verify it starts and stops without panicking
    display.stop();
}

#[test]
fn test_display_with_prefix_start_stop() {
    let progress = Arc::new(AtomicProgress::new(10, 1000));
    let display = ProgressDisplay::builder(progress)
        .with_prefix("Testing")
        .start();

    display.stop();
}

#[test]
fn test_display_with_custom_intervals() {
    let progress = Arc::new(AtomicProgress::new(10, 1000));
    let display = ProgressDisplay::builder(progress)
        .with_print_interval(250)
        .with_poll_interval(50)
        .start();

    display.stop();
}

#[test]
fn test_display_with_max_filename_len() {
    let progress = Arc::new(AtomicProgress::new(10, 1000));
    let display = ProgressDisplay::builder(progress)
        .with_max_filename_len(30)
        .start();

    display.stop();
}

#[test]
fn test_display_builder_chaining() {
    let progress = Arc::new(AtomicProgress::new(10, 1000));
    let display = ProgressDisplay::builder(progress)
        .with_prefix("Hashing")
        .with_print_interval(100)
        .with_poll_interval(25)
        .with_max_filename_len(40)
        .start();

    display.stop();
}

#[test]
fn test_display_updates_while_running() {
    let progress = Arc::new(AtomicProgress::new(10, 1000));
    let progress_clone = Arc::clone(&progress);

    let display = ProgressDisplay::builder(progress)
        .with_print_interval(50)
        .with_poll_interval(10)
        .start();

    // Simulate some work
    for i in 0..5 {
        progress_clone.add_bytes(100);
        progress_clone.set_current_file(&format!("file_{i}.txt"));
        thread::sleep(Duration::from_millis(20));
    }

    display.stop();

    // Verify progress was tracked
    assert_eq!(progress_clone.bytes_done(), 500);
}

#[test]
fn test_display_drop_stops_thread() {
    let progress = Arc::new(AtomicProgress::new(10, 1000));

    {
        let _display = ProgressDisplay::builder(progress.clone())
            .with_poll_interval(10)
            .start();
        // display goes out of scope here, Drop should stop the thread
    }

    // If we get here without hanging, the thread was stopped properly
    thread::sleep(Duration::from_millis(50));
}

#[test]
fn test_display_no_print_when_empty() {
    let progress = Arc::new(AtomicProgress::new(10, 1000));

    let display = ProgressDisplay::builder(progress)
        .with_print_interval(50)
        .with_poll_interval(10)
        .start();

    // Don't update progress - display should not print
    thread::sleep(Duration::from_millis(100));

    display.stop();
}

#[test]
fn test_multiple_displays_sequential() {
    let progress1 = Arc::new(AtomicProgress::new(10, 1000));
    let progress2 = Arc::new(AtomicProgress::new(20, 2000));

    // First display
    let display1 = ProgressDisplay::builder(progress1)
        .with_prefix("First")
        .start();
    display1.stop();

    // Second display
    let display2 = ProgressDisplay::builder(progress2)
        .with_prefix("Second")
        .start();
    display2.stop();
}
