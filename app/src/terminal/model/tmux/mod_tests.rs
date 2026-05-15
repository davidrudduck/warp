use super::*;

#[test]
fn test_parse_generator_output() {
    let input = b"^^^cmd1|||output|||0$$$";
    let result = parse_generator_output(input);
    assert!(result.is_some());
    let event = result.unwrap();
    assert_eq!(event.command_id, "cmd1");
    assert_eq!(event.output, b"output");
    assert_eq!(event.exit_code, 0);

    let input = b"^^^cmd2|||multi\nline\noutput|||255$$$";
    let result = parse_generator_output(input);
    assert!(result.is_some());
    let event = result.unwrap();
    assert_eq!(event.command_id, "cmd2");
    assert_eq!(event.output, b"multi\nline\noutput");
    assert_eq!(event.exit_code, 255);

    let input = b"invalid input";
    let result = parse_generator_output(input);
    assert!(result.is_none());
}

#[test]
fn paste_buffer_name_accepts_tmux_auto_buffer_names() {
    let name = PasteBufferName::parse(b"buffer123").expect("buffer name should parse");
    assert_eq!(name.as_str(), "buffer123");
}

#[test]
fn paste_buffer_name_rejects_empty_suffix() {
    assert_eq!(PasteBufferName::parse(b"buffer"), None);
}

#[test]
fn paste_buffer_name_rejects_non_buffer_prefix() {
    assert_eq!(PasteBufferName::parse(b"foo123"), None);
}

#[test]
fn paste_buffer_name_rejects_shell_metacharacters() {
    assert_eq!(
        PasteBufferName::parse(b"buffer1;display-message owned"),
        None
    );
}
