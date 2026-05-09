use ai::logging::DirectApiLogger;
use std::fs;
use tempfile::tempdir;

fn main() {
    println!("DirectApiLogger Demo\n");

    let temp_dir = tempdir().unwrap();
    let log_dir = temp_dir.path().join("demo-logs");

    let logger = DirectApiLogger::new(log_dir.clone());

    println!("1. Logging normal message...");
    logger.log("Starting API request to endpoint /v1/chat/completions");

    println!("2. Logging message with OpenAI API key (should be redacted)...");
    logger.log("Request with Authorization: sk-1234567890abcdefghijklmnopqrstuvwxyzABCDEFGH");

    println!("3. Logging message with Anthropic API key (should be redacted)...");
    logger.log("Using key: sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890abcdefghijklmno");

    println!("4. Logging message with Bearer token (should be redacted)...");
    logger.log("Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U");

    println!("5. Logging message with multiple secrets (should redact all)...");
    logger.log("Request: sk-test123 with Bearer abc.def.ghi to API");

    // Read and display the log file
    let log_path = log_dir.join("direct-api.log");
    let content = fs::read_to_string(log_path).unwrap();

    println!("\n=== Log File Contents ===\n{}", content);
    println!("=== Verification ===");
    println!("✓ No raw API keys in log: {}", !content.contains("sk-1234567890"));
    println!("✓ No raw Anthropic keys in log: {}", !content.contains("sk-ant-api03-abc"));
    println!("✓ No raw JWT tokens in log: {}", !content.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    println!("✓ Contains redaction markers: {}", content.contains("***REDACTED***"));
}
