// Mock authentication data
export const mockAuth = {
  data: {
    token: "mock-jwt-token-" + Math.random().toString(36).substring(7),
  },
};

// Set to true to test the onboarding flow with password setup
// Set to false to test the onboarding flow without password setup
export const mockSetupStatus = {
  required: false,
};
