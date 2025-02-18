// Use `go run foo.go` to run your program

package main

import (
	"fmt"
	"runtime"
	"sync"
)

var i = 0
var mutex sync.Mutex

func incrementing() {
	for j := 0; j < 1000000; j++ {
		mutex.Lock()
		i++

		mutex.Unlock()
	}
}

func decrementing() {
	for j := 0; j < 1000000; j++ {
		mutex.Lock()
		i--

		mutex.Unlock()
	}
}

func main() {
	// What does GOMAXPROCS do? What happens if you set it to 1?
	runtime.GOMAXPROCS(2)
	var waitgroup sync.WaitGroup
	waitgroup.Add(2)
	go func() {
		defer waitgroup.Done()
		incrementing()

	}()

	go func() {
		defer waitgroup.Done()
		decrementing()
	}()

	waitgroup.Wait()

	// We have no direct way to wait for the completion of a goroutine (without additional synchronization of some sort)
	// We will do it properly with channels soon. For now: Sleep.

	fmt.Println("The magic number is:", i)

}
