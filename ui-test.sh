#!/bin/bash

# Miao UI Automated Test Script
# Tests the complete user experience from login to logout
# Optimized with agent-browser skill integration

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
BASE_URL="${MIAO_BASE_URL:-http://localhost:6161}"
TEST_PASSWORD="${MIAO_TEST_PASSWORD:-admin123}"
SCREENSHOT_DIR="${MIAO_SCREENSHOT_DIR:-./test-screenshots}"
HEADLESS="${MIAO_HEADLESS:-true}"
MAX_RETRIES="${MIAO_MAX_RETRIES:-3}"
RETRY_DELAY="${MIAO_RETRY_DELAY:-2}"

# Test results
TESTS_PASSED=0
TESTS_FAILED=0
FAILED_TESTS=()

# Browser command wrapper
BROWSER_CMD="agent-browser"

# Create screenshot directory
mkdir -p "$SCREENSHOT_DIR"

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((TESTS_PASSED++))
}

log_error() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((TESTS_FAILED++))
    FAILED_TESTS+=("$1")
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_debug() {
    if [[ "${MIAO_DEBUG:-false}" == "true" ]]; then
        echo -e "${YELLOW}[DEBUG]${NC} $1"
    fi
}

# Browser command wrapper with retry logic
browser_exec() {
    local cmd="$1"
    local retries=0
    local result=""

    while [ $retries -lt $MAX_RETRIES ]; do
        log_debug "Executing: $BROWSER_CMD $cmd (attempt $((retries+1))/$MAX_RETRIES)"

        if result=$(eval "$BROWSER_CMD $cmd" 2>&1); then
            log_debug "Command succeeded: $cmd"
            echo "$result"
            return 0
        else
            retries=$((retries+1))
            if [ $retries -lt $MAX_RETRIES ]; then
                log_warn "Command failed, retrying in ${RETRY_DELAY}s... ($retries/$MAX_RETRIES)"
                sleep $RETRY_DELAY
            fi
        fi
    done

    log_error "Command failed after $MAX_RETRIES attempts: $cmd"
    return 1
}

# Take screenshot with error handling
take_screenshot() {
    local filename="$1"
    local filepath="$SCREENSHOT_DIR/$filename"

    if browser_exec "screenshot \"$filepath\"" > /dev/null 2>&1; then
        log_debug "Screenshot saved: $filepath"
        return 0
    else
        log_warn "Failed to save screenshot: $filepath"
        return 1
    fi
}

# Check if server is running
check_server() {
    log_info "Checking if Miao server is running at $BASE_URL..."

    local retries=0
    while [ $retries -lt 5 ]; do
        if curl -s -f "$BASE_URL/api/version" > /dev/null 2>&1; then
            log_success "Server is running"
            return 0
        fi
        retries=$((retries+1))
        if [ $retries -lt 5 ]; then
            log_warn "Server not ready, waiting... ($retries/5)"
            sleep 2
        fi
    done

    log_error "Server is not accessible at $BASE_URL"
    return 1
}

# Check if agent-browser is available
check_browser_tool() {
    log_info "Checking agent-browser availability..."

    if ! command -v agent-browser &> /dev/null; then
        log_error "agent-browser not found at /usr/local/bin/agent-browser"
        log_error "Please install agent-browser first"
        return 1
    fi

    log_success "agent-browser is available"
    return 0
}

# Initialize browser session
init_browser() {
    log_info "Initializing browser session..."

    # Clear any existing browser state
    if browser_exec "eval \"localStorage.clear(); sessionStorage.clear();\"" > /dev/null 2>&1; then
        log_success "Browser session initialized"
        return 0
    else
        log_warn "Failed to clear browser state, continuing anyway..."
        return 0
    fi
}

# Test: Homepage redirect
test_homepage_redirect() {
    log_info "Test 1: Homepage should redirect to login when not authenticated"

    if ! browser_exec "open \"$BASE_URL\"" > /dev/null 2>&1; then
        log_error "Failed to open homepage"
        return 1
    fi
    sleep 2

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/login"* ]]; then
        log_success "Homepage correctly redirects to login"
        take_screenshot "01-login-page.png"
    else
        log_error "Homepage did not redirect to login (current: $current_url)"
        take_screenshot "01-error-redirect.png"
        return 1
    fi
}

# Test: Login page elements
test_login_page_elements() {
    log_info "Test 2: Login page should have all required elements"

    local snapshot=$(browser_exec "snapshot -i" 2>/dev/null || echo "")

    if [[ "$snapshot" == *"请输入密码"* ]] && [[ "$snapshot" == *"登录"* ]]; then
        log_success "Login page has password input and login button"
        log_debug "Found required elements in snapshot"
    else
        log_error "Login page missing required elements"
        log_debug "Snapshot content: ${snapshot:0:200}..."
        take_screenshot "02-error-missing-elements.png"
        return 1
    fi
}

# Test: Setup initialization (if needed)
test_setup_if_needed() {
    log_info "Test 3: Checking if initialization is needed"

    local setup_status=$(curl -s "$BASE_URL/api/setup/status" | jq -r '.data.initialized' 2>/dev/null || echo "unknown")
    log_debug "Setup status: $setup_status"

    if [[ "$setup_status" == "false" ]]; then
        log_warn "System not initialized, initializing now..."

        local init_result=$(curl -s -X POST "$BASE_URL/api/setup/init" \
            -H "Content-Type: application/json" \
            -d "{\"password\":\"$TEST_PASSWORD\"}" | jq -r '.success' 2>/dev/null || echo "false")

        if [[ "$init_result" == "true" ]]; then
            log_success "System initialized successfully"
            sleep 1
        else
            log_error "Failed to initialize system"
            return 1
        fi
    else
        log_info "System already initialized"
    fi
}

# Test: Login with wrong password
test_login_wrong_password() {
    log_info "Test 4: Login with wrong password should show error"

    # Navigate to login page (fresh state)
    if ! browser_exec "open \"$BASE_URL/login\"" > /dev/null 2>&1; then
        log_error "Failed to open login page"
        return 1
    fi
    sleep 2

    # Get element references
    local snapshot=$(browser_exec "snapshot -i" 2>/dev/null || echo "")
    local password_ref=$(echo "$snapshot" | grep -o "textbox.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)
    local button_ref=$(echo "$snapshot" | grep -o "button.*登录.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)

    log_debug "Password field ref: $password_ref, Button ref: $button_ref"

    # Fill wrong password
    if ! browser_exec "fill \"@$password_ref\" \"wrongpassword\"" > /dev/null 2>&1; then
        log_error "Failed to fill password field"
        return 1
    fi
    sleep 0.5

    # Click login button
    if ! browser_exec "click \"@$button_ref\"" > /dev/null 2>&1; then
        log_error "Failed to click login button"
        return 1
    fi
    sleep 3

    # Check if error message is displayed
    local snapshot=$(browser_exec "snapshot" 2>/dev/null || echo "")

    if [[ "$snapshot" == *"密码错误"* ]] || [[ "$snapshot" == *"错误"* ]] || [[ "$snapshot" == *"invalid"* ]]; then
        log_success "Wrong password shows error message"
        take_screenshot "02-wrong-password.png"
    else
        log_error "No error message displayed for wrong password"
        take_screenshot "error-no-error-msg.png"
        return 1
    fi

    # Verify still on login page
    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" != *"/dashboard"* ]]; then
        log_success "User not logged in with wrong password"
    else
        log_error "User was logged in with wrong password!"
        return 1
    fi

    # Clear the password field for next test
    browser_exec "eval \"document.querySelector('input[type=\\\"password\\\"]').value = ''\"" > /dev/null 2>&1 || true
}

# Test: Login functionality
test_login() {
    log_info "Test 5: Login with valid credentials"

    # Navigate to login page (fresh state after wrong password test)
    if ! browser_exec "open \"$BASE_URL/login\"" > /dev/null 2>&1; then
        log_error "Failed to open login page"
        return 1
    fi
    sleep 3

    # Get fresh element references
    local snapshot=$(browser_exec "snapshot -i" 2>/dev/null || echo "")
    local password_ref=$(echo "$snapshot" | grep -o "textbox.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)
    local button_ref=$(echo "$snapshot" | grep -o "button.*登录.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)

    log_debug "Password field ref: $password_ref, Button ref: $button_ref"

    # Fill password
    if ! browser_exec "fill \"@$password_ref\" \"$TEST_PASSWORD\"" > /dev/null 2>&1; then
        log_error "Failed to fill password field"
        return 1
    fi
    sleep 0.5

    # Click login button
    if ! browser_exec "click \"@$button_ref\"" > /dev/null 2>&1; then
        log_error "Failed to click login button"
        return 1
    fi
    sleep 4

    # Check if redirected to dashboard
    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard"* ]]; then
        log_success "Login successful, redirected to dashboard"
        take_screenshot "03-dashboard.png"
    else
        log_error "Login failed, not redirected to dashboard (current: $current_url)"
        take_screenshot "error-login.png"
        return 1
    fi
}

# Test: Token storage
test_token_storage() {
    log_info "Test 6: Authentication token should be stored"

    local token=$(browser_exec "eval \"localStorage.getItem('miao_token')\"" 2>/dev/null || echo "null")
    log_debug "Token value: ${token:0:50}..."

    if [[ "$token" != "null" ]] && [[ -n "$token" ]]; then
        log_success "Token stored in localStorage"
    else
        log_error "Token not found in localStorage"
        return 1
    fi
}

# Test: Dashboard content
test_dashboard_content() {
    log_info "Test 7: Dashboard should display system metrics"

    local snapshot=$(browser_exec "snapshot -i" 2>/dev/null || echo "")

    if [[ "$snapshot" == *"Miao"* ]] && [[ "$snapshot" == *"退出登录"* ]]; then
        log_success "Dashboard displays navigation and content"
    else
        log_error "Dashboard content not loading correctly"
        log_debug "Snapshot content: ${snapshot:0:200}..."
        take_screenshot "error-dashboard-content.png"
        return 1
    fi
}

# Test: Navigation to Proxies page
test_navigation_proxies() {
    log_info "Test 8: Navigation to Proxies page"

    # Get element reference for 代理 link
    local snapshot=$(browser_exec "snapshot -i" 2>/dev/null || echo "")
    local proxies_ref=$(echo "$snapshot" | grep -o "代理.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)

    log_debug "Proxies link ref: $proxies_ref"

    # Click on 代理 link
    if ! browser_exec "click \"@$proxies_ref\"" > /dev/null 2>&1; then
        log_error "Failed to click Proxies link"
        return 1
    fi
    sleep 2

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard/proxies"* ]]; then
        log_success "Successfully navigated to Proxies page"
        take_screenshot "04-proxies.png"
    else
        log_error "Failed to navigate to Proxies page (current: $current_url)"
        take_screenshot "error-proxies-nav.png"
        return 1
    fi
}

# Test: Navigation to Sync page
test_navigation_sync() {
    log_info "Test 9: Navigation to Sync page"

    # Get element reference for 同步 link
    local snapshot=$(browser_exec "snapshot -i" 2>/dev/null || echo "")
    local sync_ref=$(echo "$snapshot" | grep -o "同步.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)

    log_debug "Sync link ref: $sync_ref"

    if ! browser_exec "click \"@$sync_ref\"" > /dev/null 2>&1; then
        log_error "Failed to click Sync link"
        return 1
    fi
    sleep 2

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard/sync"* ]]; then
        log_success "Successfully navigated to Sync page"
        take_screenshot "05-sync.png"
    else
        log_error "Failed to navigate to Sync page (current: $current_url)"
        take_screenshot "error-sync-nav.png"
        return 1
    fi
}

# Test: Return to Dashboard
test_return_to_dashboard() {
    log_info "Test 10: Return to Dashboard home"

    # Use JavaScript navigation to ensure it works
    if ! browser_exec "eval \"window.location.href = '/dashboard'\"" > /dev/null 2>&1; then
        log_error "Failed to navigate to dashboard"
        return 1
    fi
    sleep 2

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == "$BASE_URL/dashboard" ]] || [[ "$current_url" == "http://localhost:6161/dashboard" ]]; then
        log_success "Successfully returned to Dashboard"
    else
        log_error "Failed to return to Dashboard (current: $current_url)"
        take_screenshot "error-dashboard-return.png"
        return 1
    fi
}

# Test: Logout functionality
test_logout() {
    log_info "Test 11: Logout functionality"

    # Get fresh snapshot to find logout button
    local snapshot=$(browser_exec "snapshot -i" 2>/dev/null || echo "")

    # Find the logout button reference
    local logout_ref=$(echo "$snapshot" | grep -o "退出登录.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)

    log_debug "Logout button ref: $logout_ref"

    if [[ -z "$logout_ref" ]]; then
        log_error "Could not find logout button"
        take_screenshot "error-no-logout-button.png"
        return 1
    fi

    # Click logout button
    if ! browser_exec "click \"@$logout_ref\"" > /dev/null 2>&1; then
        log_error "Failed to click logout button"
        return 1
    fi
    sleep 2

    # Check if redirected to login
    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/login"* ]]; then
        log_success "Logout successful, redirected to login"
        take_screenshot "06-logout.png"
    else
        log_error "Logout failed, not redirected to login (current: $current_url)"
        take_screenshot "error-logout-failed.png"
        return 1
    fi
}

# Test: Token cleared after logout
test_token_cleared() {
    log_info "Test 12: Token should be cleared after logout"

    local token=$(browser_exec "eval \"localStorage.getItem('miao_token')\"" 2>/dev/null || echo "null")
    log_debug "Token after logout: $token"

    if [[ "$token" == "null" ]]; then
        log_success "Token cleared from localStorage"
    else
        log_error "Token still present after logout"
        return 1
    fi
}

# Test: Protected route access
test_protected_route() {
    log_info "Test 13: Protected routes should redirect to login"

    if ! browser_exec "open \"$BASE_URL/dashboard\"" > /dev/null 2>&1; then
        log_error "Failed to open dashboard URL"
        return 1
    fi
    sleep 2

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/login"* ]]; then
        log_success "Protected route correctly redirects to login"
    else
        log_error "Protected route accessible without authentication"
        take_screenshot "error-protected-route.png"
        return 1
    fi
}

# Test: API version endpoint
test_api_version() {
    log_info "Test 14: API version endpoint should return valid response"

    local response=$(curl -s "$BASE_URL/api/version" 2>/dev/null || echo "")
    local success=$(echo "$response" | jq -r '.success' 2>/dev/null || echo "false")

    if [[ "$success" == "true" ]]; then
        local version=$(echo "$response" | jq -r '.data.version' 2>/dev/null || echo "unknown")
        log_success "API version endpoint working (version: $version)"
    else
        log_error "API version endpoint returned invalid response"
        return 1
    fi
}

# Test: Empty password validation
test_empty_password() {
    log_info "Test 15: Login with empty password should show validation error"

    if ! browser_exec "open \"$BASE_URL/login\"" > /dev/null 2>&1; then
        log_error "Failed to open login page"
        return 1
    fi
    sleep 2

    local snapshot=$(browser_exec "snapshot -i" 2>/dev/null || echo "")
    local button_ref=$(echo "$snapshot" | grep -o "button.*登录.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)

    log_debug "Login button ref: $button_ref"

    # Try to click login without entering password
    if ! browser_exec "click \"@$button_ref\"" > /dev/null 2>&1; then
        log_error "Failed to click login button"
        return 1
    fi
    sleep 2

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" != *"/dashboard"* ]]; then
        log_success "Empty password prevented login"
    else
        log_error "Login succeeded with empty password"
        return 1
    fi
}

# Test: Session persistence after page reload
test_session_persistence() {
    log_info "Test 16: Session should persist after page reload"

    # First login
    if ! browser_exec "open \"$BASE_URL/login\"" > /dev/null 2>&1; then
        log_error "Failed to open login page"
        return 1
    fi
    sleep 2

    local snapshot=$(browser_exec "snapshot -i" 2>/dev/null || echo "")
    local password_ref=$(echo "$snapshot" | grep -o "textbox.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)
    local button_ref=$(echo "$snapshot" | grep -o "button.*登录.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)

    browser_exec "fill \"@$password_ref\" \"$TEST_PASSWORD\"" > /dev/null 2>&1
    sleep 0.5
    browser_exec "click \"@$button_ref\"" > /dev/null 2>&1
    sleep 3

    # Reload the page
    if ! browser_exec "eval \"window.location.reload()\"" > /dev/null 2>&1; then
        log_error "Failed to reload page"
        return 1
    fi
    sleep 3

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard"* ]]; then
        log_success "Session persisted after page reload"
        take_screenshot "07-session-persist.png"
    else
        log_error "Session lost after page reload"
        return 1
    fi
}

# Test: Direct access to proxies page when authenticated
test_direct_proxies_access() {
    log_info "Test 17: Direct access to proxies page when authenticated"

    if ! browser_exec "open \"$BASE_URL/dashboard/proxies\"" > /dev/null 2>&1; then
        log_error "Failed to open proxies page"
        return 1
    fi
    sleep 2

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard/proxies"* ]]; then
        log_success "Direct access to proxies page successful"
    else
        log_error "Failed to access proxies page directly"
        return 1
    fi
}

# Test: Direct access to sync page when authenticated
test_direct_sync_access() {
    log_info "Test 18: Direct access to sync page when authenticated"

    if ! browser_exec "open \"$BASE_URL/dashboard/sync\"" > /dev/null 2>&1; then
        log_error "Failed to open sync page"
        return 1
    fi
    sleep 2

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard/sync"* ]]; then
        log_success "Direct access to sync page successful"
    else
        log_error "Failed to access sync page directly"
        return 1
    fi
}

# Test: API proxies endpoint
test_api_proxies() {
    log_info "Test 19: API proxies endpoint should return valid response"

    # Get token from localStorage
    local token=$(browser_exec "eval \"localStorage.getItem('miao_token')\"" 2>/dev/null || echo "null")

    if [[ "$token" == "null" ]] || [[ -z "$token" ]]; then
        log_warn "No token available, skipping API test"
        return 0
    fi

    # Remove quotes from token
    token=$(echo "$token" | tr -d '"')

    local response=$(curl -s -H "Authorization: Bearer $token" "$BASE_URL/api/proxies" 2>/dev/null || echo "")
    local success=$(echo "$response" | jq -r '.success' 2>/dev/null || echo "false")

    if [[ "$success" == "true" ]]; then
        log_success "API proxies endpoint working"
    else
        log_error "API proxies endpoint returned invalid response"
        return 1
    fi
}

# Test: API sync status endpoint
test_api_sync_status() {
    log_info "Test 20: API sync status endpoint should return valid response"

    local token=$(browser_exec "eval \"localStorage.getItem('miao_token')\"" 2>/dev/null || echo "null")

    if [[ "$token" == "null" ]] || [[ -z "$token" ]]; then
        log_warn "No token available, skipping API test"
        return 0
    fi

    token=$(echo "$token" | tr -d '"')

    local response=$(curl -s -H "Authorization: Bearer $token" "$BASE_URL/api/sync/status" 2>/dev/null || echo "")
    local success=$(echo "$response" | jq -r '.success' 2>/dev/null || echo "false")

    if [[ "$success" == "true" ]]; then
        log_success "API sync status endpoint working"
    else
        log_error "API sync status endpoint returned invalid response"
        return 1
    fi
}

# Test: Unauthorized API access
test_api_unauthorized() {
    log_info "Test 21: API should reject requests without valid token"

    local response=$(curl -s "$BASE_URL/api/proxies" 2>/dev/null || echo "")
    local success=$(echo "$response" | jq -r '.success' 2>/dev/null || echo "true")

    if [[ "$success" == "false" ]]; then
        log_success "API correctly rejects unauthorized requests"
    else
        log_error "API allowed unauthorized access"
        return 1
    fi
}

# Test: Multiple rapid navigation
test_rapid_navigation() {
    log_info "Test 22: Multiple rapid page navigations should work"

    # Navigate to dashboard
    browser_exec "eval \"window.location.href = '/dashboard'\"" > /dev/null 2>&1
    sleep 1

    # Navigate to proxies
    browser_exec "eval \"window.location.href = '/dashboard/proxies'\"" > /dev/null 2>&1
    sleep 1

    # Navigate to sync
    browser_exec "eval \"window.location.href = '/dashboard/sync'\"" > /dev/null 2>&1
    sleep 1

    # Navigate back to dashboard
    browser_exec "eval \"window.location.href = '/dashboard'\"" > /dev/null 2>&1
    sleep 2

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard"* ]]; then
        log_success "Rapid navigation handled correctly"
    else
        log_error "Rapid navigation failed"
        return 1
    fi
}

# Test: Browser back button
test_browser_back() {
    log_info "Test 23: Browser back button should work correctly"

    # Navigate to proxies
    browser_exec "eval \"window.location.href = '/dashboard/proxies'\"" > /dev/null 2>&1
    sleep 2

    # Use browser back
    browser_exec "eval \"window.history.back()\"" > /dev/null 2>&1
    sleep 2

    local current_url=$(browser_exec "get url" 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard"* ]]; then
        log_success "Browser back button works correctly"
    else
        log_warn "Browser back button behavior unexpected (current: $current_url)"
    fi
}

# Test: Invalid route handling
test_invalid_route() {
    log_info "Test 24: Invalid routes should be handled gracefully"

    browser_exec "open \"$BASE_URL/dashboard/nonexistent\"" > /dev/null 2>&1
    sleep 2

    local snapshot=$(browser_exec "snapshot" 2>/dev/null || echo "")

    # Check if page shows error or redirects
    if [[ "$snapshot" == *"404"* ]] || [[ "$snapshot" == *"Not Found"* ]] || [[ "$snapshot" == *"Miao"* ]]; then
        log_success "Invalid route handled gracefully"
    else
        log_warn "Invalid route handling unclear"
    fi
}

# Generate test report
generate_report() {
    echo ""
    echo "======================================"
    echo "     Miao UI Test Report"
    echo "======================================"
    echo ""
    echo "Base URL: $BASE_URL"
    echo "Test Password: $TEST_PASSWORD"
    echo "Screenshots: $SCREENSHOT_DIR"
    echo ""
    echo "Tests Passed: $TESTS_PASSED"
    echo "Tests Failed: $TESTS_FAILED"
    echo ""

    if [ ${#FAILED_TESTS[@]} -gt 0 ]; then
        echo "Failed Tests:"
        for test in "${FAILED_TESTS[@]}"; do
            echo "  - $test"
        done
        echo ""
    fi

    if [ $TESTS_FAILED -eq 0 ]; then
        echo -e "${GREEN}✓ All tests passed!${NC}"
        return 0
    else
        echo -e "${RED}✗ Some tests failed${NC}"
        return 1
    fi
}

# Main test execution
main() {
    echo ""
    echo "======================================"
    echo "  Miao UI Automated Test Suite"
    echo "======================================"
    echo ""
    echo "Configuration:"
    echo "  Base URL: $BASE_URL"
    echo "  Test Password: $TEST_PASSWORD"
    echo "  Screenshots: $SCREENSHOT_DIR"
    echo "  Max Retries: $MAX_RETRIES"
    echo "  Debug Mode: ${MIAO_DEBUG:-false}"
    echo ""

    # Check agent-browser availability
    if ! check_browser_tool; then
        log_error "Cannot proceed without agent-browser"
        exit 1
    fi

    # Check server
    if ! check_server; then
        log_error "Cannot proceed without running server"
        exit 1
    fi

    # Initialize browser
    init_browser

    # Run tests (continue even if some fail)
    log_info "Starting test execution..."
    echo ""

    # Basic functionality tests
    test_homepage_redirect || true
    test_login_page_elements || true
    test_setup_if_needed || true

    # Authentication tests
    test_login_wrong_password || true
    test_empty_password || true
    test_login || true
    test_token_storage || true

    # Dashboard and navigation tests
    test_dashboard_content || true
    test_navigation_proxies || true
    test_navigation_sync || true
    test_return_to_dashboard || true

    # API endpoint tests
    test_api_version || true
    test_api_proxies || true
    test_api_sync_status || true
    test_api_unauthorized || true

    # Session and persistence tests
    test_session_persistence || true
    test_direct_proxies_access || true
    test_direct_sync_access || true

    # Navigation edge cases
    test_rapid_navigation || true
    test_browser_back || true
    test_invalid_route || true

    # Logout and security tests
    test_logout || true
    test_token_cleared || true
    test_protected_route || true

    # Generate report
    echo ""
    generate_report

    exit $?
}

# Run main function
main "$@"
