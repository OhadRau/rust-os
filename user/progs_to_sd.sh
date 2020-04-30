#!/usr/bin/env bash
if [ $# -eq 0 ]
then 
    echo no args supplied! please provide mount dir
    exit
fi

MNT=$1

PROGS=(sleep fib echo shell mkdir touch rm lsblk mount umount su ls)

for d in ${PROGS[@]}; do
    (cd $d; make build)
done

echo "copying programs to $1/bin/"
sudo mkdir $MNT/bin/
for d in ${PROGS[@]}; do
    echo "copying $d..."
    sudo cp $d/build/$d.bin $MNT/bin/$d
done
echo "unmounting $MNT..."
sudo umount $MNT
echo bye
