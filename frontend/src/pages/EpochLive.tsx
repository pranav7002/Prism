import EpochTimeline from "@/components/prism/EpochTimeline";
import ProofPipeline from "@/components/prism/ProofPipeline";
import SignalLedger from "@/components/prism/SignalLedger";

const EpochLive = () => {
  return (
    <>
      <section className="container mx-auto pt-12 pb-8">
        <div className="mb-8">
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Route · /epoch/live</p>
          <h1 className="display text-4xl md:text-5xl mt-2">Operations Explorer</h1>
        </div>
        <EpochTimeline />
      </section>

      <section className="container mx-auto pb-12">
        <ProofPipeline />
      </section>

      <section className="container mx-auto pb-24">
        <div className="glass p-8 md:p-12">
          <SignalLedger />
        </div>
      </section>
    </>
  );
};

export default EpochLive;