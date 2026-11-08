use super::*;
use std::{
    sync::{Barrier, mpsc},
    time::{Duration, Instant},
};

fn svg(color: &str, size: usize) -> Vec<u8> {
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" height=\"{size}\"><rect width=\"100%\" height=\"100%\" fill=\"{color}\"/></svg>").into_bytes()
}
fn ready(
    loader: &WebSvgLoader,
    ctx: &egui::Context,
    uri: &str,
    hint: SizeHint,
) -> Arc<egui::ColorImage> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match loader.load(ctx, uri, hint).unwrap() {
            ImagePoll::Ready { image } => return image,
            ImagePoll::Pending { .. } if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(5))
            }
            _ => panic!("SVG stayed pending"),
        }
    }
}

#[test]
fn an_invalidated_render_cannot_resurrect_or_overwrite_the_new_image() {
    for forget_all in [false, true] {
        let loader = WebSvgLoader::default();
        // Hold an admitted real loader job before completion, without a production test hook.
        let (sender, receiver) = mpsc::sync_channel(QUEUED);
        assert!(loader.jobs.set(Ok(sender)).is_ok());
        let ctx = egui::Context::default();
        let uri = "file:///generation.svg";
        ctx.include_bytes(uri, svg("red", 16));
        assert!(matches!(
            loader.load(&ctx, uri, SizeHint::default()).unwrap(),
            ImagePoll::Pending { .. }
        ));
        let old = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let (thread_entered, thread_release, cache) =
            (entered.clone(), release.clone(), loader.cache.clone());
        let old_render = std::thread::spawn(move || {
            thread_entered.wait();
            thread_release.wait();
            complete(&cache, old);
        });
        entered.wait();
        if forget_all {
            loader.forget_all();
            ctx.forget_all_images();
        } else {
            loader.forget(uri);
            ctx.forget_image(uri);
        }
        // egui's byte store keeps the first bytes for a URI until it is forgotten.
        ctx.include_bytes(uri, svg("blue", 16));
        assert!(matches!(
            loader.load(&ctx, uri, SizeHint::default()).unwrap(),
            ImagePoll::Pending { .. }
        ));
        let new = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        complete(&loader.cache, new);
        let new_image = ready(&loader, &ctx, uri, SizeHint::default());
        assert_eq!(new_image.pixels[0], egui::Color32::BLUE);
        release.wait();
        old_render.join().unwrap();
        let still_new = ready(&loader, &ctx, uri, SizeHint::default());
        assert!(Arc::ptr_eq(&new_image, &still_new));
        loader.forget(uri);
        loader.load(&ctx, uri, SizeHint::default()).unwrap();
        let forgotten = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        loader.forget(uri);
        complete(&loader.cache, forgotten);
        assert!(
            !loader
                .cache
                .lock()
                .unwrap()
                .entries
                .contains_key(&(uri.into(), SizeHint::default()))
        );
    }
}

#[test]
fn queue_admission_and_retained_images_stay_within_both_budgets() {
    let ctx = egui::Context::default();
    let held = WebSvgLoader::default();
    let (sender, receiver) = mpsc::sync_channel(QUEUED);
    assert!(held.jobs.set(Ok(sender)).is_ok());
    for index in 0..40 {
        let uri = format!("file:///queued-{index}.svg");
        ctx.include_bytes(uri.clone(), svg("blue", 8));
        held.load(&ctx, &uri, SizeHint::default()).unwrap();
    }
    assert_eq!(receiver.try_iter().count(), QUEUED);
    assert_eq!(held.cache.lock().unwrap().entries.len(), QUEUED);
    let loader = WebSvgLoader::default();
    for index in 0..150 {
        let uri = format!("file:///cached-{index}.svg");
        ctx.include_bytes(uri.clone(), svg("blue", 8));
        let hint = SizeHint::Width(1 + (index % 3) * 256);
        ready(&loader, &ctx, &uri, hint);
        let cache = loader.cache.lock().unwrap();
        assert!(cache.entries.len() <= CACHE_ENTRIES);
        assert!(cache.bytes() <= CACHE_BYTES);
    }
    for index in 0..10 {
        let uri = format!("file:///large-{index}.svg");
        ctx.include_bytes(uri.clone(), svg("blue", 1024));
        ready(&loader, &ctx, &uri, SizeHint::default());
        assert!(loader.byte_size() <= CACHE_BYTES);
    }
}

#[test]
fn oversized_and_expanding_svgs_complete_as_visible_loader_errors() {
    let loader = WebSvgLoader::default();
    let ctx = egui::Context::default();
    for (index, source) in [
        svg("red", 100_000),
        b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><g id='a'><use href='#b'/></g><g id='b'><use href='#a'/></g><use href='#a'/></svg>".to_vec(),
        vec![b' '; SOURCE_BYTES + 1],
    ].into_iter().enumerate() {
        let uri = format!("file:///oversized-{index}.svg");
        ctx.include_bytes(uri.clone(), source);
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            match loader.load(&ctx, &uri, SizeHint::default()) {
                Err(LoadError::Loading(error)) => { assert!(!error.is_empty()); break; }
                Ok(ImagePoll::Pending { .. }) if Instant::now() < until => std::thread::sleep(Duration::from_millis(5)),
                Ok(_) => panic!("expected a completed visible error, got an image poll"),
                Err(error) => panic!("expected a completed visible error, got {error}"),
            }
        }
    }
}
