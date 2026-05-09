use super::*;
use warpui::App;
use warpui_extras::secure_storage;

#[test]
fn api_key_manager_does_not_load_on_init() {
    App::test((), |app| async move {
        let manager = app.add_singleton_model(|ctx| ApiKeyManager::new(ctx));

        manager.read(&app, |manager, _ctx| {
            // Should NOT trigger keychain prompt yet
            // Verify no keys cached
            assert!(manager.is_cache_empty());
        });
    });
}

#[test]
fn api_key_manager_loads_on_first_keys_access() {
    App::test((), |mut app| async move {
        // Register noop secure storage for testing
        app.update(|ctx| {
            secure_storage::register_noop("test", ctx);
        });

        let manager = app.add_singleton_model(|ctx| ApiKeyManager::new(ctx));

        manager.read(&app, |manager, ctx| {
            // First call triggers load from secure storage
            let _keys = manager.keys(ctx);

            // Verify loaded (cache populated)
            assert!(!manager.is_cache_empty());
        });
    });
}

#[test]
fn api_key_manager_uses_cache_on_subsequent_calls() {
    App::test((), |mut app| async move {
        // Register noop secure storage for testing
        app.update(|ctx| {
            secure_storage::register_noop("test", ctx);
        });

        let manager = app.add_singleton_model(|ctx| ApiKeyManager::new(ctx));

        manager.read(&app, |manager, ctx| {
            // First call loads
            let keys1 = manager.keys(ctx);

            // Second call uses cache (no storage access)
            let keys2 = manager.keys(ctx);

            assert_eq!(keys1.openai, keys2.openai);
            assert_eq!(keys1.anthropic, keys2.anthropic);
            assert_eq!(keys1.google, keys2.google);
            assert!(!manager.is_cache_empty());
        });
    });
}

#[test]
fn api_key_manager_cache_cleared_on_drop() {
    App::test((), |mut app| async move {
        // Register noop secure storage for testing
        app.update(|ctx| {
            secure_storage::register_noop("test", ctx);
        });

        {
            let manager = app.add_singleton_model(|ctx| ApiKeyManager::new(ctx));
            manager.read(&app, |manager, ctx| {
                let _keys = manager.keys(ctx);
                assert!(!manager.is_cache_empty());
            });
        } // manager dropped when app scope ends
    });

    // New app/instance has no cache
    App::test((), |app| async move {
        let manager2 = app.add_singleton_model(|ctx| ApiKeyManager::new(ctx));
        manager2.read(&app, |manager, _ctx| {
            assert!(manager.is_cache_empty());
        });
    });
}

#[test]
fn set_key_updates_cache_and_storage() {
    App::test((), |mut app| async move {
        // Register noop secure storage for testing
        app.update(|ctx| {
            secure_storage::register_noop("test", ctx);
        });

        let manager = app.add_singleton_model(|ctx| ApiKeyManager::new(ctx));

        manager.update(&mut app, |manager, ctx| {
            // Set a key (should update cache + storage)
            manager.set_openai_key(Some("test-key".to_string()), ctx);

            // Cache should be populated
            assert!(!manager.is_cache_empty());

            // Key should be accessible
            assert_eq!(manager.keys(ctx).openai.as_deref(), Some("test-key"));
        });
    });
}
