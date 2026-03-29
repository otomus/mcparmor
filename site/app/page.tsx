import { HeroSection } from "@/components/hero/HeroSection";
import { ProblemSection } from "@/components/sections/ProblemSection";
import { HowItWorks } from "@/components/sections/HowItWorks";
import { ProtectionTable } from "@/components/sections/ProtectionTable";
import { AdversarialMatrix } from "@/components/sections/AdversarialMatrix";
import { PlatformCards } from "@/components/sections/PlatformCards";
import { HostSupport } from "@/components/sections/HostSupport";
import { ArqitectCallout } from "@/components/sections/ArqitectCallout";
import { RuntimeIndication } from "@/components/sections/RuntimeIndication";
import { ForAuthorsBuilders } from "@/components/sections/ForAuthorsBuilders";
import { InstallSection } from "@/components/sections/InstallSection";
import { TestEvidence } from "@/components/sections/TestEvidence";

/**
 * Homepage — the sell, the missing link, the proof.
 *
 * Sections from DESIGN.md, assembled in order. TestEvidence sits
 * between the adversarial matrix and platform cards to reinforce
 * engineering credibility with real test counts.
 */
export default function HomePage() {
  return (
    <>
      <HeroSection />
      <ProblemSection />
      <HowItWorks />
      <ProtectionTable />
      <AdversarialMatrix />
      <TestEvidence />
      <PlatformCards />
      <HostSupport />
      <RuntimeIndication />
      <ArqitectCallout />
      <ForAuthorsBuilders />
      <InstallSection />
    </>
  );
}
