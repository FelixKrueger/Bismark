import csv,sys,statistics as st
rows=list(csv.DictReader(open(sys.argv[1]),delimiter="\t"))
print("rows",len(rows),"| distinct SAM-body md5:",len({r["body_md5"] for r in rows}),"| load1 range",min(float(r["load1"]) for r in rows),"-",max(float(r["load1"]) for r in rows))
for t in sorted({int(r["threads"]) for r in rows}):
    out={}
    for e in ("bwa-mem3","bwa-mem4"):
        rs=[r for r in rows if r["engine"]==e and int(r["threads"])==t]
        cpu=[float(r["user_s"])+float(r["sys_s"]) for r in rs]; wall=[float(r["wall_s"]) for r in rs]; rss=[int(r["max_rss_bytes"])/1e6 for r in rs]
        out[e]=(cpu,wall,rss)
        print(f"t={t} {e} n={len(rs)}: CPU med {st.median(cpu):.2f}s (min {min(cpu):.2f}, max {max(cpu):.2f}) | wall med {st.median(wall):.2f}s ({min(wall):.2f}-{max(wall):.2f}) | maxRSS med {st.median(rss):.0f} MB ({min(rss):.0f}-{max(rss):.0f})")
    a,b=out["bwa-mem3"],out["bwa-mem4"]
    print(f"   mem4/mem3 medians: CPU {st.median(b[0])/st.median(a[0]):.2f}  min-CPU {min(b[0])/min(a[0]):.2f}  wall {st.median(b[1])/st.median(a[1]):.2f}  RSS {st.median(b[2])/st.median(a[2]):.2f}")
