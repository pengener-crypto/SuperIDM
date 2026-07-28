package com.superidm.di

import com.superidm.data.db.AppDatabase
import com.superidm.data.db.DownloadStatsDao
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object Phase3Module {
    
    @Provides
    @Singleton
    fun provideDownloadStatsDao(db: AppDatabase): DownloadStatsDao {
        return db.downloadStatsDao()
    }
}
