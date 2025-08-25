# BitchatX Scrolling Fix

## Problem Identified
Users reported that text disappears when half of the window is filled up. Specifically, when typing `/help` multiple times in the system channel, the whole window clears and won't show anything else.

## Root Cause Analysis
After investigating the code, I identified several issues with the scrolling system:

### 1. Dual Scrolling Mechanisms
The app had **two different scrolling mechanisms working against each other**:
- **App-level scrolling**: In `get_visible_messages()` - this filtered which messages were passed to UI
- **Widget-level scrolling**: In the `Paragraph::new().scroll()` call - this scrolled the rendered widget

This caused conflicts where the app would filter messages and then the widget would try to scroll differently.

### 2. Incorrect Message Filtering Logic
The `get_visible_messages()` method had complex logic that tried to pre-filter messages before passing them to the UI:
```rust
let start = self.scroll_offset.min(messages.len().saturating_sub(height));
let end = (start + height).min(messages.len());
```
This logic was error-prone and didn't work well with the widget-level scrolling.

### 3. Missing Autoscroll for System Messages
When system messages were added via `add_status_message()` and `add_message_to_current_channel()`, they bypassed the normal message processing pipeline that handles autoscroll. This meant that when help text or status messages were added, the view wouldn't automatically scroll to show them.

### 4. Complex Autoscroll Logic
The `scroll_to_bottom()` method had overly complex logic with buffers and thresholds that didn't always work correctly.

## Fixes Implemented

### 1. Simplified Message Display (`src/app.rs`)
**Before**: App-level filtering in `get_visible_messages()`
```rust
pub fn get_visible_messages(&self, height: usize) -> Vec<&Message> {
    if let Some(channel) = self.get_current_channel() {
        let messages = &channel.messages;
        let start = self.scroll_offset.min(messages.len().saturating_sub(height));
        let end = (start + height).min(messages.len());
        messages[start..end].iter().collect()
    } else {
        vec![]
    }
}
```

**After**: Return all messages, let UI widget handle scrolling
```rust
pub fn get_visible_messages(&self, _height: usize) -> Vec<&Message> {
    if let Some(channel) = self.get_current_channel() {
        // Return all messages - let the UI widget handle scrolling
        channel.messages.iter().collect()
    } else {
        vec![]
    }
}
```

### 2. Simplified Scrolling Logic (`src/app.rs`)
**Before**: Complex logic with buffers and thresholds
```rust
let scroll_buffer = 3; // Allow 3 extra messages before scrolling
let scroll_threshold = visible_height + scroll_buffer;

if message_count > scroll_threshold {
    self.scroll_offset = message_count.saturating_sub(visible_height);
} else {
    self.scroll_offset = 0;
}
```

**After**: Simple, direct logic
```rust
if message_count > visible_height {
    self.scroll_offset = message_count.saturating_sub(visible_height);
} else {
    self.scroll_offset = 0;
}
```

### 3. Added Autoscroll for System Messages (`src/app.rs`)
**Modified `add_message_to_current_channel()`**:
```rust
pub fn add_message_to_current_channel(&mut self, message: String) {
    // ... existing code to create and add message ...
    
    // Trigger autoscroll since we added a new message
    if self.should_autoscroll {
        self.scroll_to_bottom();
    }
}
```

**Modified `add_status_message()`**:
```rust
pub fn add_status_message(&mut self, message: String) {
    // ... existing code to create and add message ...
    
    // Trigger autoscroll if we're in system channel
    if self.current_channel.as_deref() == Some(&self.system_channel) && self.should_autoscroll {
        self.scroll_to_bottom();
    }
}
```

### 4. Enhanced Help Display (`src/app.rs`)
**Modified `show_help()`**:
```rust
async fn show_help(&mut self) {
    // Enable autoscroll to ensure help text is visible
    self.should_autoscroll = true;
    
    // ... existing help text ...
    
    for line in help_text {
        self.add_message_to_current_channel(line);
    }
    
    // Ensure we scroll to bottom after adding help text
    self.scroll_to_bottom();
}
```

### 5. Improved Autoscroll Detection (`src/app.rs`)
**Enhanced `update_autoscroll_status()`**:
```rust
fn update_autoscroll_status(&mut self) {
    if let Some(channel) = self.get_current_channel() {
        let message_count = channel.messages.len();
        let visible_height = 25;
        
        // If we're at or near bottom, re-enable auto-scrolling
        let bottom_threshold = message_count.saturating_sub(visible_height);
        if self.scroll_offset >= bottom_threshold.saturating_sub(5) {
            self.should_autoscroll = true;
        }
    }
}
```

## Technical Details

### Scrolling Architecture
**Before Fix**:
```
Messages → get_visible_messages() [filtering] → UI → Paragraph.scroll() [widget scrolling]
                                                    ↑ Conflicts!
```

**After Fix**:
```
Messages → get_visible_messages() [no filtering] → UI → Paragraph.scroll() [widget scrolling]
                                                           ✓ Single mechanism
```

### Message Flow for System Messages
**Before Fix**:
```
/help → show_help() → add_message_to_current_channel() → ChannelManager → No autoscroll
                                                                              ↑ Issue!
```

**After Fix**:
```
/help → show_help() → add_message_to_current_channel() → ChannelManager → scroll_to_bottom()
                                                                              ✓ Fixed!
```

## Testing Instructions

1. **Build the application**:
   ```bash
   cargo build --release
   ```

2. **Test scrolling scenarios**:
   ```bash
   ./target/release/bitchatx --no-logo
   ```

3. **Specific test cases**:
   - **Help text test**: Type `/help` multiple times
     - ✅ Help text should appear and scroll correctly
     - ✅ Window should not clear or hide text
     - ✅ Subsequent `/help` commands should append to existing text
   
   - **Manual scrolling test**: Use Page Up/Down, Arrow keys
     - ✅ Manual scrolling should work smoothly
     - ✅ Auto-scroll should disable when manually scrolling up
     - ✅ Auto-scroll should re-enable when scrolling to bottom
   
   - **Mixed content test**: Send regular messages and system messages
     - ✅ All message types should display correctly
     - ✅ Auto-scroll should work for all message types
     - ✅ No text should disappear unexpectedly

## Expected Results

After this fix, users should experience:
- ✅ **Persistent text display**: Messages no longer disappear when window fills up
- ✅ **Consistent help display**: `/help` command works reliably every time
- ✅ **Smooth scrolling**: Manual and automatic scrolling work harmoniously
- ✅ **No more clearing**: Window doesn't clear unexpectedly
- ✅ **Proper autoscroll**: System messages (help, status) auto-scroll correctly
- ✅ **Reliable navigation**: Page Up/Down work as expected

## Files Modified

- `src/app.rs` - Major changes to scrolling logic and message handling
- `src/ui/mod.rs` - No changes needed (widget scrolling was already correct)

## Backward Compatibility

All changes maintain backward compatibility:
- Existing keyboard shortcuts work the same
- Channel switching behavior unchanged
- Message sending/receiving logic preserved
- Only scrolling and display logic improved

## Rollback Plan

If issues arise, revert these specific changes:
1. Restore original `get_visible_messages()` logic
2. Restore original `scroll_to_bottom()` complexity
3. Remove autoscroll triggers from `add_message_to_current_channel()` and `add_status_message()`
4. Remove autoscroll enable from `show_help()`

The changes are self-contained and don't affect core messaging functionality.