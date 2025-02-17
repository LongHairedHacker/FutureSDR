#!/bin/bash

outfile=perf-data/results.csv
rm -f ${outfile}

echo "sdr,run,pipes,stages,samples,max_copy,scheduler,time" > ${outfile}

files=$(ls perf-data/gr_*.csv 2>/dev/null || echo)
for f in ${files}
do
	echo "gr,$(cat $f)" >> ${outfile}
done

files=$(ls perf-data/nr_*.csv 2>/dev/null || echo)
for f in ${files}
do
	echo "nr,$(cat $f)" >> ${outfile}
done
