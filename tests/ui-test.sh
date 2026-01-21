#!/bin/bash

# Miao UI Automated Test Script
# Tests the complete user experience from login to logout

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

# Test results
TESTS_PASSED=0
TESTS_FAILED=0
FAILED_TESTS=()

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

# Check if server is running
check_server() {
    log_info "Checking if Miao server is running at $BASE_URL..."

    if curl -s -f "$BASE_URL/api/version" > /dev/null 2>&1; then
        log_success "Server is running"
        return 0
    else
        log_error "Server is not accessible at $BASE_URL"
        return 1
    fi
}

# Initialize browser session
init_browser() {
    log_info "Initializing browser session..."

    # Clear any existing browser state
    agent-browser eval "localStorage.clear(); sessionStorage.clear();" 2>/dev/null || true

    log_success "Browser session initialized"
}

# Test: Homepage redirect
test_homepage_redirect() {
    log_info "Test 1: Homepage should redirect to login when not authenticated"

    agent-browser open "$BASE_URL" > /dev/null 2>&1
    sleep 2

    local current_url=$(agent-browser get url 2>/dev/null || echo "")

    if [[ "$current_url" == *"/login"* ]]; then
        log_success "Homepage correctly redirects to login"
        agent-browser screenshot "$SCREENSHOT_DIR/01-login-page.png" > /dev/null 2>&1
    else
        log_error "Homepage did not redirect to login (current: $current_url)"
        agent-browser screenshot "$SCREENSHOT_DIR/01-error-redirect.png" > /dev/null 2>&1
        return 1
    fi
}

# Test: Login page elements
test_login_page_elements() {
    log_info "Test 2: Login page should have all required elements"

    local snapshot=$(agent-browser snapshot -i 2>/dev/null || echo "")

    if [[ "$snapshot" == *"请输入密码"* ]] && [[ "$snapshot" == *"登录"* ]]; then
        log_success "Login page has password input and login button"
    else
        log_error "Login page missing required elements"
        return 1
    fi
}

# Test: Setup initialization (if needed)
test_setup_if_needed() {
    log_info "Test 3: Checking if initialization is needed"

    local setup_status=$(curl -s "$BASE_URL/api/setup/status" | jq -r '.data.initialized' 2>/dev/null || echo "unknown")

    if [[ "$setup_status" == "false" ]]; then
        log_warn "System not initialized, initializing now..."

        local init_result=$(curl -s -X POST "$BASE_URL/api/setup/init" \
            -H "Content-Type: application/json" \
            -d "{\"password\":\"$TEST_PASSWORD\"}" | jq -r '.success' 2>/dev/null || echo "false")

        if [[ "$init_result" == "true" ]]; then
            log_success "System initialized successfully"
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
    agent-browser open "$BASE_URL/login" > /dev/null 2>&1
    sleep 2

    # Fill wrong password
    agent-browser fill @e1 "wrongpassword" > /dev/null 2>&1
    sleep 0.5

    # Click login button
    agent-browser click @e2 > /dev/null 2>&1
    sleep 3

    # Check if error message is displayed
    local snapshot=$(agent-browser snapshot 2>/dev/null || echo "")

    if [[ "$snapshot" == *"密码错误"* ]] || [[ "$snapshot" == *"错误"* ]] || [[ "$snapshot" == *"invalid"* ]]; then
        log_success "Wrong password shows error message"
        agent-browser screenshot "$SCREENSHOT_DIR/02-wrong-password.png" 2>/dev/null || true
    else
        log_error "No error message displayed for wrong password"
        agent-browser screenshot "$SCREENSHOT_DIR/error-no-error-msg.png" 2>/dev/null || true
        return 1
    fi

    # Verify still on login page
    local current_url=$(agent-browser get url 2>/dev/null || echo "")

    if [[ "$current_url" != *"/dashboard"* ]]; then
        log_success "User not logged in with wrong password"
    else
        log_error "User was logged in with wrong password!"
        return 1
    fi

    # Clear the password field for next test
    agent-browser eval "document.querySelector('input[type=\"password\"]').value = ''" > /dev/null 2>&1 || true
}

# Test: Login functionality
test_login() {
    log_info "Test 5: Login with valid credentials"

    # Navigate to login page (fresh state after wrong password test)
    agent-browser open "$BASE_URL/login" > /dev/null 2>&1
    sleep 3

    # Get fresh element references
    local snapshot=$(agent-browser snapshot -i 2>/dev/null || echo "")
    local password_ref=$(echo "$snapshot" | grep -o "textbox.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)
    local button_ref=$(echo "$snapshot" | grep -o "button.*登录.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)

    # Fill password
    agent-browser fill "@$password_ref" "$TEST_PASSWORD" > /dev/null 2>&1
    sleep 0.5

    # Click login button
    agent-browser click "@$button_ref" > /dev/null 2>&1
    sleep 4

    # Check if redirected to dashboard
    local current_url=$(agent-browser get url 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard"* ]]; then
        log_success "Login successful, redirected to dashboard"
        agent-browser screenshot "$SCREENSHOT_DIR/03-dashboard.png" > /dev/null 2>&1
    else
        log_error "Login failed, not redirected to dashboard (current: $current_url)"
        agent-browser screenshot "$SCREENSHOT_DIR/error-login.png" > /dev/null 2>&1
        return 1
    fi
}

# Test: Token storage
test_token_storage() {
    log_info "Test 6: Authentication token should be stored"

    local token=$(agent-browser eval "localStorage.getItem('miao_token')" 2>/dev/null || echo "null")

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

    local snapshot=$(agent-browser snapshot -i 2>/dev/null || echo "")

    if [[ "$snapshot" == *"Miao"* ]] && [[ "$snapshot" == *"退出登录"* ]]; then
        log_success "Dashboard displays navigation and content"
    else
        log_error "Dashboard content not loading correctly"
        return 1
    fi
}

# Test: Navigation to Proxies page
test_navigation_proxies() {
    log_info "Test 8: Navigation to Proxies page"

    # Click on 代理 link
    agent-browser click @e2 > /dev/null 2>&1
    sleep 2

    local current_url=$(agent-browser get url 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard/proxies"* ]]; then
        log_success "Successfully navigated to Proxies page"
        agent-browser screenshot "$SCREENSHOT_DIR/03-proxies.png" > /dev/null 2>&1
    else
        log_error "Failed to navigate to Proxies page (current: $current_url)"
        return 1
    fi
}

# Test: Navigation to Sync page
test_navigation_sync() {
    log_info "Test 9: Navigation to Sync page"

    agent-browser click @e3 > /dev/null 2>&1
    sleep 2

    local current_url=$(agent-browser get url 2>/dev/null || echo "")

    if [[ "$current_url" == *"/dashboard/sync"* ]]; then
        log_success "Successfully navigated to Sync page"
        agent-browser screenshot "$SCREENSHOT_DIR/04-sync.png" > /dev/null 2>&1
    else
        log_error "Failed to navigate to Sync page (current: $current_url)"
        return 1
    fi
}

# Test: Return to Dashboard
test_return_to_dashboard() {
    log_info "Test 10: Return to Dashboard home"

    # Use JavaScript navigation to ensure it works
    agent-browser eval "window.location.href = '/dashboard'" > /dev/null 2>&1
    sleep 2

    local current_url=$(agent-browser get url 2>/dev/null || echo "")

    if [[ "$current_url" == "$BASE_URL/dashboard" ]] || [[ "$current_url" == "http://localhost:6161/dashboard" ]]; then
        log_success "Successfully returned to Dashboard"
    else
        log_error "Failed to return to Dashboard (current: $current_url)"
        agent-browser screenshot "$SCREENSHOT_DIR/error-dashboard-return.png" 2>/dev/null || true
        return 1
    fi
}

# Test: Logout functionality
test_logout() {
    log_info "Test 11: Logout functionality"

    # Get fresh snapshot to find logout button
    local snapshot=$(agent-browser snapshot -i 2>/dev/null || echo "")

    # Find the logout button reference
    local logout_ref=$(echo "$snapshot" | grep -o "退出登录.*\[ref=e[0-9]\+\]" | grep -o "e[0-9]\+" | head -1)

    if [[ -z "$logout_ref" ]]; then
        log_error "Could not find logout button"
        agent-browser screenshot "$SCREENSHOT_DIR/error-no-logout-button.png" 2>/dev/null || true
        return 1
    fi

    # Click logout button
    agent-browser click "@$logout_ref" > /dev/null 2>&1
    sleep 2

    # Check if redirected to login
    local current_url=$(agent-browser get url 2>/dev/null || echo "")

    if [[ "$current_url" == *"/login"* ]]; then
        log_success "Logout successful, redirected to login"
        agent-browser screenshot "$SCREENSHOT_DIR/05-logout.png" 2>/dev/null || true
    else
        log_error "Logout failed, not redirected to login (current: $current_url)"
        agent-browser screenshot "$SCREENSHOT_DIR/error-logout-failed.png" 2>/dev/null || true
        return 1
    fi
}

# Test: Token cleared after logout
test_token_cleared() {
    log_info "Test 12: Token should be cleared after logout"

    local token=$(agent-browser eval "localStorage.getItem('miao_token')" 2>/dev/null || echo "null")

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

    agent-browser open "$BASE_URL/dashboard" > /dev/null 2>&1
    sleep 2

    local current_url=$(agent-browser get url 2>/dev/null || echo "")

    if [[ "$current_url" == *"/login"* ]]; then
        log_success "Protected route correctly redirects to login"
    else
        log_error "Protected route accessible without authentication"
        return 1
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

    # Check server
    if ! check_server; then
        log_error "Cannot proceed without running server"
        exit 1
    fi

    # Initialize browser
    init_browser

    # Run tests (continue even if some fail)
    test_homepage_redirect || true
    test_login_page_elements || true
    test_setup_if_needed || true
    test_login_wrong_password || true
    test_login || true
    test_token_storage || true
    test_dashboard_content || true
    test_navigation_proxies || true
    test_navigation_sync || true
    test_return_to_dashboard || true
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
