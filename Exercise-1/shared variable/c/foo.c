// Compile with `gcc foo.c -Wall -std=gnu99 -lpthread`, or use the makefile
// The executable will be named `foo` if you use the makefile, or `a.out` if you use gcc directly

#include <pthread.h>
#include <stdio.h>

int i = 0;
pthread_mutex_t taken_flag; 

// Note the return type: void*
void* incrementingThreadFunction(){
    for(int j = 0; j < 1000000; j++){
        pthread_mutex_lock(&taken_flag);
        i++;
        pthread_mutex_unlock(&taken_flag);
    }
    return NULL;
}

void* decrementingThreadFunction(){
    for(int j = 0; j < 1000000; j++){
        pthread_mutex_lock(&taken_flag);
        i--;
        pthread_mutex_unlock(&taken_flag);
    }
    return NULL;
}


int main(){
    // TODO: 
    // start the two functions as their own threads using `pthread_create`
    pthread_mutex_init(&taken_flag,NULL);
    pthread_t thread1;
    pthread_t thread2;
    pthread_create(&thread1,NULL, incrementingThreadFunction, NULL);
    pthread_create(&thread2,NULL, decrementingThreadFunction, NULL);
    // Hint: search the web! Maybe try "pthread_create example"?
    
    // TODO:
    // wait for the two threads to be done before printing the final result
    // Hint: Use `pthread_join`    
    pthread_join(thread1, NULL);
    pthread_join(thread2, NULL);
    printf("The magic number is hei: %d\n", i);
    pthread_mutex_destroy(&taken_flag);
    return 0;
}
