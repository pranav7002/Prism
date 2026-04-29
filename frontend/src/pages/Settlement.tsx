import CooperativeMEV from "@/components/prism/CooperativeMEV";
import ShapleyBreakdown from "@/components/prism/ShapleyBreakdown";
import SettlementPipeline from "@/components/prism/SettlementPipeline";

const Settlement = () => {
  return (
    <>
      <section className="container mx-auto pt-12 pb-6">
        <div className="mb-8">
          <p className="mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Route · /settlement</p>
          <h1 className="display text-4xl md:text-5xl mt-2">Value Distribution</h1>
        </div>
      </section>

      <section className="container mx-auto pb-8">
        <SettlementPipeline />
      </section>

      <section className="container mx-auto pb-8">
        <CooperativeMEV />
      </section>

      <section className="container mx-auto pb-10">
        <ShapleyBreakdown />
      </section>
    </>
  );
};

export default Settlement;