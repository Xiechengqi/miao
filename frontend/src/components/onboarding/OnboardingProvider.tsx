"use client";

import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  ReactNode,
} from "react";
import { api } from "@/lib/api";

const ONBOARDING_COMPLETED_KEY = "miao_onboarding_completed";
const ONBOARDING_STEP_KEY = "miao_onboarding_step";

interface OnboardingContextValue {
  /** Whether the onboarding tour should be shown */
  showOnboarding: boolean;
  /** Whether password setup is required (first time) */
  setupRequired: boolean;
  /** Current step index (0-based) */
  currentStep: number;
  /** Whether the onboarding state is loading */
  isLoading: boolean;
  /** Save the current step progress */
  setCurrentStep: (step: number) => void;
  /** Mark onboarding as completed */
  completeOnboarding: () => void;
  /** Reset onboarding to show again (for re-tour) */
  resetOnboarding: () => void;
  /** Start the onboarding tour */
  startOnboarding: () => void;
}

const OnboardingContext = createContext<OnboardingContextValue | null>(null);

export function useOnboarding() {
  const context = useContext(OnboardingContext);
  if (!context) {
    throw new Error("useOnboarding must be used within an OnboardingProvider");
  }
  return context;
}

interface OnboardingProviderProps {
  children: ReactNode;
}

export function OnboardingProvider({ children }: OnboardingProviderProps) {
  const [isLoading, setIsLoading] = useState(true);
  const [setupRequired, setSetupRequired] = useState(false);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [currentStep, setCurrentStepState] = useState(0);

  // Check onboarding status on mount
  useEffect(() => {
    const checkOnboardingStatus = async () => {
      try {
        // Check if onboarding was already completed
        const completed = localStorage.getItem(ONBOARDING_COMPLETED_KEY);
        if (completed === "true") {
          setShowOnboarding(false);
          setIsLoading(false);
          return;
        }

        // Check if this is a fresh setup (needs password)
        const token = localStorage.getItem("miao_token");
        if (token) {
          api.setToken(token);
        }

        try {
          const { required } = await api.checkSetupRequired();
          setSetupRequired(required);

          // If setup is required or onboarding not completed, show tour
          if (required || !completed) {
            // Restore step progress if available
            const savedStep = localStorage.getItem(ONBOARDING_STEP_KEY);
            if (savedStep) {
              const step = parseInt(savedStep, 10);
              if (!isNaN(step) && step >= 0) {
                setCurrentStepState(step);
              }
            }
            setShowOnboarding(true);
          }
        } catch {
          // If API fails (e.g., not authenticated), don't show onboarding
          setShowOnboarding(false);
        }
      } catch {
        // Ignore errors, don't show onboarding
        setShowOnboarding(false);
      } finally {
        setIsLoading(false);
      }
    };

    checkOnboardingStatus();
  }, []);

  const setCurrentStep = useCallback((step: number) => {
    setCurrentStepState(step);
    localStorage.setItem(ONBOARDING_STEP_KEY, String(step));
  }, []);

  const completeOnboarding = useCallback(() => {
    localStorage.setItem(ONBOARDING_COMPLETED_KEY, "true");
    localStorage.removeItem(ONBOARDING_STEP_KEY);
    setShowOnboarding(false);
    setSetupRequired(false);
  }, []);

  const resetOnboarding = useCallback(() => {
    localStorage.removeItem(ONBOARDING_COMPLETED_KEY);
    localStorage.removeItem(ONBOARDING_STEP_KEY);
    setCurrentStepState(0);
    setShowOnboarding(true);
  }, []);

  const startOnboarding = useCallback(() => {
    setCurrentStepState(0);
    setShowOnboarding(true);
  }, []);

  return (
    <OnboardingContext.Provider
      value={{
        showOnboarding,
        setupRequired,
        currentStep,
        isLoading,
        setCurrentStep,
        completeOnboarding,
        resetOnboarding,
        startOnboarding,
      }}
    >
      {children}
    </OnboardingContext.Provider>
  );
}
