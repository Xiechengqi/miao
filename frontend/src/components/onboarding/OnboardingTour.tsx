"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import { usePathname, useRouter } from "next/navigation";
import { driver, DriveStep, Driver } from "driver.js";
import "driver.js/dist/driver.css";
import { useOnboarding } from "./OnboardingProvider";
import { api } from "@/lib/api";

// Total number of steps in the tour
const TOTAL_STEPS = 5;

// Password validation
const MIN_PASSWORD_LENGTH = 4;

export function OnboardingTour() {
  const pathname = usePathname();
  const router = useRouter();
  const {
    showOnboarding,
    setupRequired,
    currentStep,
    isLoading,
    setCurrentStep,
    completeOnboarding,
  } = useOnboarding();

  const driverRef = useRef<Driver | null>(null);
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [passwordError, setPasswordError] = useState("");
  const [isSettingPassword, setIsSettingPassword] = useState(false);
  const [mounted, setMounted] = useState(false);

  // Track if we should start the tour
  const shouldStartTour =
    mounted &&
    !isLoading &&
    showOnboarding &&
    pathname?.startsWith("/dashboard");

  // Build steps based on whether setup is required
  const buildSteps = useCallback((): DriveStep[] => {
    const steps: DriveStep[] = [];

    // Step 1: Welcome & Password Setup (only if setup required)
    if (setupRequired) {
      steps.push({
        popover: {
          title: "欢迎使用 Miao 控制面板",
          description: `
            <div style="text-align: center; padding-top: 1rem;">
              <div class="onboarding-icon">
                <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <rect width="18" height="18" x="3" y="3" rx="2" ry="2"/>
                  <path d="M7 7h.01"/><path d="M17 7h.01"/><path d="M7 17h.01"/><path d="M17 17h.01"/>
                </svg>
              </div>
              <p style="margin-bottom: 1rem; color: #64748b;">首次使用需要设置管理员密码</p>
              <div class="onboarding-password-form">
                <input
                  type="password"
                  id="onboarding-password"
                  class="onboarding-password-input"
                  placeholder="设置密码（至少 ${MIN_PASSWORD_LENGTH} 位）"
                />
                <input
                  type="password"
                  id="onboarding-confirm-password"
                  class="onboarding-password-input"
                  placeholder="确认密码"
                />
                <div id="onboarding-password-error" class="onboarding-password-error"></div>
                <p class="onboarding-password-hint">此密码用于登录控制面板</p>
              </div>
            </div>
          `,
          showButtons: ["next"],
          nextBtnText: "设置密码",
          onNextClick: async () => {
            const pwdInput = document.getElementById(
              "onboarding-password"
            ) as HTMLInputElement;
            const confirmInput = document.getElementById(
              "onboarding-confirm-password"
            ) as HTMLInputElement;
            const errorDiv = document.getElementById(
              "onboarding-password-error"
            );

            const pwd = pwdInput?.value || "";
            const confirm = confirmInput?.value || "";

            // Validate
            if (pwd.length < MIN_PASSWORD_LENGTH) {
              if (errorDiv) errorDiv.textContent = `密码至少 ${MIN_PASSWORD_LENGTH} 位`;
              return;
            }
            if (pwd !== confirm) {
              if (errorDiv) errorDiv.textContent = "两次输入的密码不一致";
              return;
            }

            // Show loading state
            const nextBtn = document.querySelector(
              ".driver-popover-next-btn"
            ) as HTMLButtonElement;
            if (nextBtn) {
              nextBtn.classList.add("loading");
              nextBtn.disabled = true;
            }

            try {
              // Call API to set password
              await api.setup(pwd);

              // Login with the new password
              await api.login(pwd);

              // Clear error and move to next step
              if (errorDiv) errorDiv.textContent = "";
              setCurrentStep(1);
              driverRef.current?.moveNext();
            } catch (error) {
              if (errorDiv) {
                errorDiv.textContent =
                  error instanceof Error ? error.message : "设置失败，请重试";
              }
            } finally {
              if (nextBtn) {
                nextBtn.classList.remove("loading");
                nextBtn.disabled = false;
              }
            }
          },
        },
      });
    } else {
      // Welcome without password setup
      steps.push({
        popover: {
          title: "欢迎使用 Miao 控制面板",
          description: `
            <div style="text-align: center; padding-top: 1rem;">
              <div class="onboarding-icon">
                <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <rect width="18" height="18" x="3" y="3" rx="2" ry="2"/>
                  <path d="M7 7h.01"/><path d="M17 7h.01"/><path d="M7 17h.01"/><path d="M17 17h.01"/>
                </svg>
              </div>
              <p style="color: #64748b;">让我们快速了解控制面板的主要功能</p>
            </div>
          `,
          showButtons: ["next"],
          nextBtnText: "开始引导",
        },
      });
    }

    // Step 2: Dashboard Overview
    steps.push({
      element: '[data-onboarding="dashboard-overview"]',
      popover: {
        title: "系统概览",
        description:
          "这里展示系统的整体运行状态，包括 CPU、内存使用情况，以及性能趋势图表。您可以随时查看系统的健康状况。",
        side: "bottom",
        align: "center",
      },
    });

    // Step 3: Proxy Management
    steps.push({
      element: '[data-onboarding="nav-proxies"]',
      popover: {
        title: "代理管理",
        description:
          "这是核心功能区域。您可以在这里管理代理节点、切换代理组、测试节点延迟，以及配置订阅源。",
        side: "right",
        align: "start",
      },
    });

    // Step 4: Navigation Overview
    steps.push({
      element: '[data-onboarding="nav-section"]',
      popover: {
        title: "功能导航",
        description:
          "侧边栏包含所有功能模块：主机管理、代理设置、穿透服务、终端、桌面远程、应用管理、备份同步和系统日志。",
        side: "right",
        align: "center",
      },
    });

    // Step 5: Completion
    steps.push({
      popover: {
        title: "准备就绪！",
        description: `
          <div style="text-align: center; padding-top: 1rem;">
            <div class="onboarding-success-icon">
              <svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
            </div>
            <p style="color: #64748b; margin-bottom: 0.5rem;">引导已完成，您现在可以开始使用 Miao 控制面板了。</p>
            <p style="color: #94a3b8; font-size: 0.85rem;">如需重新查看引导，可在侧边栏点击"使用引导"按钮。</p>
          </div>
        `,
        showButtons: ["next"],
        nextBtnText: "开始使用",
        onNextClick: () => {
          completeOnboarding();
          driverRef.current?.destroy();
        },
      },
    });

    return steps;
  }, [setupRequired, setCurrentStep, completeOnboarding]);

  // Initialize driver instance
  useEffect(() => {
    setMounted(true);
    return () => {
      if (driverRef.current) {
        driverRef.current.destroy();
        driverRef.current = null;
      }
    };
  }, []);

  // Start tour when conditions are met
  useEffect(() => {
    if (!shouldStartTour) return;

    // Small delay to ensure DOM is ready
    const timer = setTimeout(() => {
      const steps = buildSteps();

      driverRef.current = driver({
        showProgress: true,
        progressText: "{{current}} / {{total}}",
        allowClose: true,
        stagePadding: 10,
        stageRadius: 12,
        animate: true,
        smoothScroll: true,
        disableActiveInteraction: false,
        popoverClass: "driver-popover",
        overlayColor: "rgba(15, 23, 42, 0.15)",
        steps,
        onCloseClick: () => {
          // Ask for confirmation when closing early
          if (currentStep < TOTAL_STEPS - 1) {
            const confirmed = window.confirm(
              "确定要跳过引导吗？您可以稍后在侧边栏重新开始引导。"
            );
            if (confirmed) {
              completeOnboarding();
              driverRef.current?.destroy();
            }
          } else {
            completeOnboarding();
            driverRef.current?.destroy();
          }
        },
        onNextClick: () => {
          // Default next behavior (for steps without custom onNextClick)
          const newStep = currentStep + 1;
          setCurrentStep(newStep);
          driverRef.current?.moveNext();
        },
        onPrevClick: () => {
          const newStep = Math.max(0, currentStep - 1);
          setCurrentStep(newStep);
          driverRef.current?.movePrevious();
        },
        onDestroyStarted: () => {
          // Cleanup
        },
      });

      // Start from saved step or beginning
      driverRef.current.drive(currentStep);
    }, 500);

    return () => {
      clearTimeout(timer);
    };
  }, [
    shouldStartTour,
    buildSteps,
    currentStep,
    setCurrentStep,
    completeOnboarding,
  ]);

  // Don't render anything visible
  return null;
}
